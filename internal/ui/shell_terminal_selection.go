package ui

import (
	"math"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"
)

type terminalPoint struct{ x, y int }
type terminalSelection struct {
	anchor, head           terminalPoint
	down, selected, remote bool
	pointer                fyne.Position
	modifiers              fyne.KeyModifier
	animation              *fyne.Animation
	lastScroll             time.Time
}

var _ fyne.Draggable = (*terminalSurface)(nil)
var _ desktop.Mouseable = (*terminalSurface)(nil)

func (t *terminalSurface) MouseDown(event *desktop.MouseEvent) {
	if t.closed || event.Button != desktop.MouseButtonPrimary {
		return
	}
	t.clearSelection()
	t.Tapped(&event.PointEvent)
	s := &t.selection
	s.down = true
	s.pointer, s.modifiers = event.Position, event.Modifier
	s.remote = len(t.mouseModes) > 0 && event.Modifier&fyne.KeyModifierShift == 0
	if s.remote {
		t.sendPointer(event.Position, 0)
		return
	}
	s.anchor = t.selectionPoint(event.Position)
	s.head = s.anchor
}
func (t *terminalSurface) MouseUp(event *desktop.MouseEvent) {
	if event.Button != desktop.MouseButtonPrimary {
		return
	}
	t.endSelectionDrag()
}
func (t *terminalSurface) Dragged(event *fyne.DragEvent) {
	s := &t.selection
	if t.closed || !s.down {
		return
	}
	s.pointer = event.Position
	if s.remote {
		t.sendPointer(event.Position, 1)
		return
	}
	s.head = t.selectionPoint(event.Position)
	s.selected = true
	t.updateSelectionScroll()
	t.Refresh()
}
func (t *terminalSurface) DragEnd() { t.endSelectionDrag() }
func (t *terminalSurface) endSelectionDrag() {
	s := &t.selection
	if s.down && s.remote && !t.closed {
		t.sendPointer(s.pointer, 2)
	}
	s.down = false
	t.stopSelectionScroll()
}
func (t *terminalSurface) clearSelection() {
	hadSelection := t.selection.selected
	t.endSelectionDrag()
	t.selection = terminalSelection{}
	if hadSelection {
		t.Refresh()
	}
}

// Absolute row coordinates keep a selection attached to the same history when
// the viewport scrolls. Wide-cell continuation columns snap to their glyph.
func (t *terminalSurface) selectionPoint(position fyne.Position) terminalPoint {
	cell := t.cellSize()
	x := min(t.emulator.Width()-1, max(0, int(math.Floor(float64(position.X/cell.Width)))))
	y := min(t.emulator.Height()-1, max(0, int(math.Floor(float64(position.Y/cell.Height)))))
	row := t.emulator.ScrollbackLen() - t.scroll + y
	for x > 0 {
		c := t.absoluteCell(x, row)
		if c == nil || c.Width != 0 || c.Content != "" {
			break
		}
		x--
	}
	return terminalPoint{x, row}
}
func (t *terminalSurface) absoluteCell(x, y int) *uv.Cell {
	history := t.emulator.ScrollbackLen()
	if y < history {
		return t.emulator.ScrollbackCellAt(x, y)
	}
	return t.emulator.CellAt(x, y-history)
}
func pointBefore(a, b terminalPoint) bool { return a.y < b.y || a.y == b.y && a.x < b.x }
func (t *terminalSurface) selectionBounds() (terminalPoint, terminalPoint) {
	start, end := t.selection.anchor, t.selection.head
	if pointBefore(end, start) {
		start, end = end, start
	}
	width := 1
	if c := t.absoluteCell(end.x, end.y); c != nil {
		width = max(1, c.Width)
	}
	end.x = min(t.emulator.Width(), end.x+width)
	return start, end
}
func (t *terminalSurface) selectedCell(x, y int) bool {
	if !t.selection.selected {
		return false
	}
	start, end := t.selectionBounds()
	point := terminalPoint{x, t.emulator.ScrollbackLen() - t.scroll + y}
	return !pointBefore(point, start) && pointBefore(point, end)
}
func (t *terminalSurface) selectedText() string {
	if t.emulator == nil || !t.selection.selected {
		return ""
	}
	start, end := t.selectionBounds()
	var result strings.Builder
	for y := start.y; y <= end.y; y++ {
		from, to := 0, t.emulator.Width()
		if y == start.y {
			from = start.x
		}
		if y == end.y {
			to = end.x
		}
		var line strings.Builder
		for x := from; x < to; {
			cell := t.absoluteCell(x, y)
			if cell == nil || cell.Content == "" {
				line.WriteByte(' ')
				x++
				continue
			}
			line.WriteString(cell.Content)
			x += max(1, cell.Width)
		}
		if y > start.y {
			result.WriteByte('\n')
		}
		result.WriteString(strings.TrimRight(line.String(), " "))
	}
	return result.String()
}
func (t *terminalSurface) copyText() string {
	if t.selection.selected {
		return t.selectedText()
	}
	return t.emulator.String()
}

func (t *terminalSurface) sendPointer(position fyne.Position, kind int) {
	if kind == 1 && !t.mouseModes[ansi.ModeMouseButtonEvent] && !t.mouseModes[ansi.ModeMouseAnyEvent] {
		return
	}
	if kind == 2 && !t.mouseModes[ansi.ModeMouseNormal] && !t.mouseModes[ansi.ModeMouseHighlight] && !t.mouseModes[ansi.ModeMouseButtonEvent] && !t.mouseModes[ansi.ModeMouseAnyEvent] {
		return
	}
	cell := t.cellSize()
	mouse := uv.Mouse{X: min(t.emulator.Width()-1, max(0, int(position.X/cell.Width))), Y: min(t.emulator.Height()-1, max(0, int(position.Y/cell.Height))), Button: uv.MouseLeft, Mod: terminalModifiers(t.selection.modifiers)}
	switch kind {
	case 0:
		t.emulator.SendMouse(uv.MouseClickEvent(mouse))
	case 1:
		t.emulator.SendMouse(uv.MouseMotionEvent(mouse))
	case 2:
		t.emulator.SendMouse(uv.MouseReleaseEvent(mouse))
	}
}
