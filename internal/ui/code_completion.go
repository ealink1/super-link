package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

type codeCompletion struct {
	entry     *codeEntry
	request   func(bool)
	popup     *widget.PopUp
	overlay   fyne.CanvasObject
	keyboard  *completionKeyboard
	scroll    *container.Scroll
	rows      []*completionRow
	items     []sqlworkbench.CompletionCandidate
	query     sqlworkbench.CompletionQuery
	source    string
	cursor    int
	selected  int
	requested bool
}

func (c *codeCompletion) hide() {
	c.requested = false
	c.removePopup()
	c.items, c.rows = nil, nil
	c.scroll = nil
	c.keyboard = nil
}

func (c *codeCompletion) removePopup() {
	if c.popup == nil {
		return
	}
	canvas := c.popup.Canvas
	var above []fyne.CanvasObject
	for i, overlay := range canvas.Overlays().List() {
		if overlay == c.overlay {
			above = append(above, canvas.Overlays().List()[i+1:]...)
			break
		}
	}
	focused := canvas.Focused()
	// Fyne removes every overlay above a dismissed popup. Restore unrelated
	// overlays in their original order, along with the current dialog's focus.
	c.popup.Hide()
	c.popup, c.overlay = nil, nil
	for _, overlay := range above {
		canvas.Overlays().Add(overlay)
	}
	if len(above) > 0 && focused != nil {
		canvas.Focus(focused)
	}
}

func (c *codeCompletion) visible() bool {
	return c.popup != nil && c.popup.Visible() && len(c.items) > 0
}

func (c *codeCompletion) trigger(manual bool) {
	if c.entry.Disabled() || c.entry.SelectedText() != "" {
		c.hide()
		return
	}
	c.requested = true
	c.request(manual)
}

func (c *codeCompletion) show(q sqlworkbench.CompletionQuery, items []sqlworkbench.CompletionCandidate) {
	if !c.requested {
		return
	}
	canvas := fyne.CurrentApp().Driver().CanvasForObject(c.entry)
	if canvas == nil || canvas.Focused() != c.entry && canvas.Focused() != c.keyboard {
		c.hide()
		return
	}
	c.removePopup()
	if len(items) == 0 {
		c.items = nil
		return
	}
	e := c.entry
	c.query, c.items, c.selected = q, items, 0
	c.source, c.cursor = e.Text, e.CursorTextOffset()
	c.rows = nil
	objects := make([]fyne.CanvasObject, 0, len(items))
	for i, item := range items {
		row := newCompletionRow(item, i == 0, func() { c.selected = i; c.accept() })
		c.rows = append(c.rows, row)
		objects = append(objects, row)
	}
	c.scroll = container.NewVScroll(container.NewVBox(objects...))
	c.keyboard = &completionKeyboard{entry: e, content: c.scroll}
	c.keyboard.ExtendBaseWidget(c.keyboard)
	c.popup = widget.NewPopUp(c.keyboard, canvas)
	height := min(float32(min(8, len(items)))*(28+theme.Padding())+8, max(36, canvas.Size().Height-8))
	width := min(float32(390), canvas.Size().Width-8)
	c.popup.Resize(fyne.NewSize(width, height))
	position := fyne.CurrentApp().Driver().AbsolutePositionForObject(e).Add(e.CursorPosition())
	if e.viewport != nil {
		position = position.Subtract(e.viewport.Offset)
	}
	line := fyne.MeasureText("M", theme.TextSize(), e.TextStyle).Height
	position.Y += line + 4
	if position.Y+height > canvas.Size().Height {
		position.Y -= height + line + 8
	}
	position.X = max(4, min(position.X, canvas.Size().Width-width-4))
	position.Y = max(4, position.Y)
	c.popup.ShowAtPosition(position)
	c.overlay = canvas.Overlays().Top()
	canvas.Focus(c.keyboard)
}

func (c *codeCompletion) key(key fyne.KeyName) bool {
	if !c.visible() {
		return false
	}
	switch key {
	case fyne.KeyEscape:
		c.hide()
	case fyne.KeyUp, fyne.KeyDown:
		step := 1
		if key == fyne.KeyUp {
			step = -1
		}
		c.rows[c.selected].selected = false
		c.rows[c.selected].Refresh()
		c.selected = (c.selected + step + len(c.items)) % len(c.items)
		c.rows[c.selected].selected = true
		c.rows[c.selected].Refresh()
		y := c.rows[c.selected].Position().Y
		if y < c.scroll.Offset.Y {
			c.scroll.ScrollToOffset(fyne.NewPos(0, y))
		} else if y+28 > c.scroll.Offset.Y+c.scroll.Size().Height {
			c.scroll.ScrollToOffset(fyne.NewPos(0, y+28-c.scroll.Size().Height))
		}
	case fyne.KeyReturn, fyne.KeyEnter, fyne.KeyTab:
		c.accept()
	default:
		return false
	}
	return true
}

func (c *codeCompletion) accept() {
	e := c.entry
	if !c.visible() || e.Text != c.source || e.CursorTextOffset() != c.cursor || e.SelectedText() != "" || e.Disabled() {
		c.hide()
		return
	}
	insert, q := c.items[c.selected].Insert, c.query
	c.hide()
	for i := c.cursor; i < q.End; i++ {
		e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyRight})
	}
	if q.End > q.Start {
		e.Entry.KeyDown(&fyne.KeyEvent{Name: desktop.KeyShiftLeft})
		for i := q.Start; i < q.End; i++ {
			e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyLeft})
		}
		e.Entry.KeyUp(&fyne.KeyEvent{Name: desktop.KeyShiftLeft})
	}
	// Native paste keeps the existing undo stack and avoids the system clipboard.
	e.Entry.TypedShortcut(&fyne.ShortcutPaste{Clipboard: &completionClipboard{text: insert}})
	fyne.CurrentApp().Driver().CanvasForObject(e).Focus(e)
}

type completionClipboard struct{ text string }

func (c *completionClipboard) Content() string         { return c.text }
func (c *completionClipboard) SetContent(value string) { c.text = value }

func (e *codeEntry) TypedRune(value rune) {
	e.Entry.TypedRune(value)
	if e.completion != nil {
		e.completion.trigger(false)
	}
}

func (e *codeEntry) TypedKey(key *fyne.KeyEvent) {
	if e.completion != nil && e.completion.key(key.Name) {
		return
	}
	e.Entry.TypedKey(key)
	if e.completion != nil {
		if key.Name == fyne.KeyBackspace || key.Name == fyne.KeyDelete {
			e.completion.trigger(false)
		} else {
			e.completion.hide()
		}
	}
}

func (e *codeEntry) KeyDown(key *fyne.KeyEvent) {
	if e.completion != nil && (key.Name == desktop.KeyShiftLeft || key.Name == desktop.KeyShiftRight) {
		e.completion.hide()
	}
	e.Entry.KeyDown(key)
}

func (e *codeEntry) FocusLost() {
	if e.completion != nil {
		e.completion.hide()
	}
	e.Entry.FocusLost()
}

func (e *codeEntry) MouseDown(event *desktop.MouseEvent) {
	if e.completion != nil {
		e.completion.hide()
	}
	e.Entry.MouseDown(event)
}

func (e *codeEntry) completionShortcut(shortcut fyne.Shortcut) bool {
	if e.completion == nil {
		return false
	}
	if key, ok := shortcut.(fyne.KeyboardShortcut); ok && key.Key() == fyne.KeySpace && key.Mod() == fyne.KeyModifierControl {
		e.completion.trigger(true)
		return true
	}
	e.completion.hide()
	return false
}
