package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

// Rows deliberately do not take keyboard focus away from the editor.
type completionRow struct {
	widget.BaseWidget
	item     sqlworkbench.CompletionCandidate
	selected bool
	choose   func()
}

func newCompletionRow(item sqlworkbench.CompletionCandidate, selected bool, choose func()) *completionRow {
	r := &completionRow{item: item, selected: selected, choose: choose}
	r.ExtendBaseWidget(r)
	return r
}

func (r *completionRow) Tapped(*fyne.PointEvent) { r.choose() }

func (r *completionRow) CreateRenderer() fyne.WidgetRenderer {
	background := canvas.NewRectangle(color.Transparent)
	name := widget.NewLabel(r.item.Text)
	name.Truncation = fyne.TextTruncateEllipsis
	detail := widget.NewLabel(r.item.Kind + " · " + r.item.Detail)
	detail.Importance = widget.LowImportance
	detail.Truncation = fyne.TextTruncateEllipsis
	content := container.NewBorder(nil, nil, nil, container.NewGridWrap(fyne.NewSize(130, 28), detail), name)
	return &completionRowRenderer{row: r, background: background, content: content}
}

type completionRowRenderer struct {
	row        *completionRow
	background *canvas.Rectangle
	content    fyne.CanvasObject
}

func (r *completionRowRenderer) MinSize() fyne.Size { return fyne.NewSize(260, 28) }
func (r *completionRowRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.content}
}
func (r *completionRowRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.content.Resize(size)
}
func (r *completionRowRenderer) Refresh() {
	r.background.FillColor = color.Transparent
	if r.row.selected {
		r.background.FillColor = theme.Color(theme.ColorNameSelection)
	}
	r.background.Refresh()
	r.content.Refresh()
}
func (r *completionRowRenderer) Destroy() {}

// Fyne gives every overlay its own focus manager. Forward its keyboard events
// to the editor so that typing continues while the candidate popup is open.
type completionKeyboard struct {
	widget.BaseWidget
	entry   *codeEntry
	content fyne.CanvasObject
}

func (k *completionKeyboard) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(k.content)
}
func (k *completionKeyboard) FocusGained()                  {}
func (k *completionKeyboard) FocusLost()                    {}
func (k *completionKeyboard) AcceptsTab() bool              { return true }
func (k *completionKeyboard) TypedRune(r rune)              { k.entry.TypedRune(r) }
func (k *completionKeyboard) TypedKey(key *fyne.KeyEvent)   { k.entry.TypedKey(key) }
func (k *completionKeyboard) TypedShortcut(s fyne.Shortcut) { k.entry.TypedShortcut(s) }
func (k *completionKeyboard) KeyDown(key *fyne.KeyEvent)    { k.entry.KeyDown(key) }
func (k *completionKeyboard) KeyUp(key *fyne.KeyEvent)      { k.entry.KeyUp(key) }
