package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"strings"
)

func (e *noteEntry) hitSource(pos fyne.Position) int {
	best, distance := 0, float32(1e20)
	for _, p := range e.richRenderer.points {
		dy := p.position.Y + p.height/2 - pos.Y
		dx := p.position.X - pos.X
		d := dy*dy*4 + dx*dx
		if d < distance {
			best = p.offset
			distance = d
		}
	}
	return best
}
func (e *noteEntry) MouseDown(event *desktop.MouseEvent) {
	if e.richRenderer == nil || !e.rich {
		e.Entry.MouseDown(event)
		if !e.richShift {
			e.selectionAnchor = e.sourceOffset()
		}
		return
	}
	if e.Disabled() {
		return
	}
	fyne.CurrentApp().Driver().CanvasForObject(e).Focus(e)
	target := e.hitSource(event.Position)
	if e.richShift {
		e.selectSource(e.selectionAnchor, target)
	} else {
		// A zero-width Shift selection clears any preceding selection through the
		// public Entry keyboard API, without depending on Fyne's private fields.
		e.selectSource(target, target)
		e.selectionAnchor = target
	}
	e.Refresh()
}
func (e *noteEntry) selectSource(anchor, target int) {
	e.selectionAnchor = anchor
	e.Entry.KeyUp(&fyne.KeyEvent{Name: desktop.KeyShiftLeft})
	e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyRight})
	e.setSourceOffset(anchor)
	e.Entry.KeyDown(&fyne.KeyEvent{Name: desktop.KeyShiftLeft})
	if target > 0 {
		e.setSourceOffset(target - 1)
		e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyRight})
	} else {
		e.setSourceOffset(1)
		e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyLeft})
	}
	e.Entry.KeyUp(&fyne.KeyEvent{Name: desktop.KeyShiftLeft})
}
func (e *noteEntry) Dragged(event *fyne.DragEvent) {
	if e.richRenderer == nil || !e.rich {
		e.Entry.Dragged(event)
		return
	}
	if e.Disabled() {
		return
	}
	e.selectSource(e.selectionAnchor, e.hitSource(event.Position))
	e.Refresh()
}
func (e *noteEntry) KeyDown(event *fyne.KeyEvent) {
	if event.Name == desktop.KeyShiftLeft || event.Name == desktop.KeyShiftRight {
		if !e.richShift {
			e.selectionAnchor = e.sourceOffset()
		}
		e.richShift = true
	}
	e.Entry.KeyDown(event)
}
func (e *noteEntry) KeyUp(event *fyne.KeyEvent) {
	if event.Name == desktop.KeyShiftLeft || event.Name == desktop.KeyShiftRight {
		e.richShift = false
	}
	e.Entry.KeyUp(event)
}
func (e *noteEntry) TypedKey(event *fyne.KeyEvent) {
	if e.richRenderer == nil || !e.rich {
		e.Entry.TypedKey(event)
		return
	}
	if e.Disabled() {
		return
	}
	if (event.Name == fyne.KeyReturn || event.Name == fyne.KeyEnter) && e.continueNoteParagraph() {
		return
	}
	if event.Name == fyne.KeyBackspace || event.Name == fyne.KeyDelete {
		if e.richDelete(event.Name) {
			return
		}
	}
	if (event.Name == fyne.KeyLeft || event.Name == fyne.KeyRight) && (e.SelectedText() == "" || e.richShift) {
		current := e.sourceOffset()
		target := current
		for _, p := range e.richRenderer.points {
			if event.Name == fyne.KeyRight && p.offset > current && (target == current || p.offset < target) {
				target = p.offset
			}
			if event.Name == fyne.KeyLeft && p.offset < current && (target == current || p.offset > target) {
				target = p.offset
			}
		}
		if e.richShift {
			e.selectSource(e.selectionAnchor, target)
		} else {
			e.selectSource(target, target)
			e.selectionAnchor = target
		}
		e.Refresh()
		return
	}
	if event.Name == fyne.KeyUp || event.Name == fyne.KeyDown {
		current := e.richRenderer.nearestOffset(e.sourceOffset())
		y := current.position.Y + current.height/2
		if event.Name == fyne.KeyUp {
			y -= current.height + 12
		} else {
			y += current.height + 12
		}
		target := e.hitSource(fyne.NewPos(current.position.X, y))
		if e.richShift {
			e.selectSource(e.selectionAnchor, target)
		} else {
			e.selectSource(target, target)
			e.selectionAnchor = target
		}
		e.Refresh()
		return
	}
	e.Entry.TypedKey(event)
}

// Delete visible characters rather than the Markdown delimiter beside them.
func (e *noteEntry) richDelete(key fyne.KeyName) bool {
	if e.SelectedText() != "" {
		return false
	}
	offset := e.sourceOffset()
	total := len([]rune(e.Text))
	candidate := -1
	lines := notePresentation(e.Text)
	blockStart, blockEnd, body, inCode := noteEnclosingCodeBlock(e.Text, offset, offset)
	if inCode && strings.TrimSpace(body) == "" {
		e.replaceFormatRange(blockStart, blockEnd, "", 0, 0)
		return true
	}
	for index, line := range lines {
		if line.fence {
			continue
		}
		if inCode && (line.start <= blockStart || line.end >= blockEnd) {
			continue
		}
		for _, run := range line.runs {
			for i := range []rune(run.text) {
				at := run.start + i
				if key == fyne.KeyBackspace && at < offset {
					candidate = at
				}
				if key == fyne.KeyDelete && at >= offset && candidate < 0 {
					candidate = at
				}
			}
		}
		// Newlines adjacent to hidden fences are structural boundaries, not
		// editable characters. Deleting them would expose ``` inside the code.
		boundary := index+1 < len(lines) && lines[index+1].fence
		if line.end < total && !boundary {
			if key == fyne.KeyBackspace && line.end < offset {
				candidate = line.end
			}
			if key == fyne.KeyDelete && line.end >= offset && candidate < 0 {
				candidate = line.end
			}
		}
	}
	if inCode && candidate >= 0 && candidate < blockStart+4 {
		return true
	}
	if candidate < 0 {
		return true
	}
	e.selectSource(candidate, candidate+1)
	e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	return true
}
