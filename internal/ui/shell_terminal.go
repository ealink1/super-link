package ui

import (
	"image/color"
	"io"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/vt"
)

type terminalSurface struct {
	widget.BaseWidget
	imeCaret      fyne.CanvasObject
	emulator      *vt.Emulator // All parser, input-encoding and screen calls stay on the UI goroutine.
	resize        func(int, int)
	closed        bool
	focused       bool
	visibleCursor bool
	altScreen     bool
	scroll        int
	rejectPaste   func()
	textSize      float32
}

func newTerminalSurface(resize func(int, int)) *terminalSurface {
	t := &terminalSurface{emulator: vt.NewEmulator(80, 24), resize: resize, visibleCursor: true, textSize: 14}
	t.emulator.SetScrollbackSize(1000)
	t.emulator.SetCallbacks(vt.Callbacks{CursorVisibility: func(visible bool) { t.visibleCursor = visible }, AltScreen: func(active bool) { t.altScreen = active; t.scroll = 0 }})
	t.ExtendBaseWidget(t)
	return t
}

func (t *terminalSurface) feed(output []byte) {
	if !t.closed {
		before := t.emulator.ScrollbackLen()
		_, _ = t.emulator.Write(output)
		if t.scroll > 0 {
			t.scroll = min(t.emulator.ScrollbackLen(), t.scroll+max(0, t.emulator.ScrollbackLen()-before))
		}
		t.Refresh()
	}
}

func (t *terminalSurface) dispose() {
	if t.closed {
		return
	}
	t.closed = true
	t.resize = nil
	t.rejectPaste = nil
	// Close the concurrency-safe pipe rather than racing Emulator.Close's state
	// against Read. The joined pipe consumer never mutates emulator screen state.
	if closer, ok := t.emulator.InputPipe().(io.Closer); ok {
		_ = closer.Close()
	}
}

func (t *terminalSurface) Tapped(*fyne.PointEvent) {
	if parent := fyne.CurrentApp().Driver().CanvasForObject(t); parent != nil {
		parent.Focus(t)
	}
}
func (t *terminalSurface) AcceptsTab() bool { return true }
func (t *terminalSurface) FocusGained()     { t.focused = true; t.Refresh() }
func (t *terminalSurface) FocusLost()       { t.focused = false; t.Refresh() }

func (t *terminalSurface) CreateRenderer() fyne.WidgetRenderer {
	background := canvas.NewRectangle(resolveShellColor(terminalBackground))
	cursor := canvas.NewRectangle(terminalCursorColor())
	t.imeCaret = cursor
	r := &terminalRenderer{terminal: t, background: background, cursor: cursor, text: &fyne.Container{}}
	r.Refresh()
	return r
}

const terminalBackground = shellBackground
const terminalForeground = shellTextColor

type terminalRenderer struct {
	terminal    *terminalSurface
	background  *canvas.Rectangle
	cursor      *canvas.Rectangle
	text        *fyne.Container
	pool        []*canvas.Text
	backgrounds []*canvas.Rectangle
}

func (t *terminalSurface) cellSize() fyne.Size {
	return fyne.MeasureText("M", t.textSize, fyne.TextStyle{Monospace: true})
}

func (t *terminalSurface) setTextSize(size float32) {
	if t.textSize == size || t.closed {
		return
	}
	t.textSize = size
	cell := t.cellSize()
	width, height := t.Size().Width, t.Size().Height
	cols, rows := min(500, max(2, int(width/cell.Width))), min(200, max(2, int(height/cell.Height)))
	t.emulator.Resize(cols, rows)
	if t.resize != nil {
		t.resize(cols, rows)
	}
	t.Refresh()
}

func (r *terminalRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.text.Resize(size)
	cell := r.terminal.cellSize()
	cols := min(500, max(2, int(size.Width/cell.Width)))
	rows := min(200, max(2, int(size.Height/cell.Height)))
	e := r.terminal.emulator
	if cols != e.Width() || rows != e.Height() {
		e.Resize(cols, rows)
		if r.terminal.resize != nil {
			r.terminal.resize(cols, rows)
		}
		r.Refresh()
	}
}
func (r *terminalRenderer) MinSize() fyne.Size { return fyne.NewSize(240, 160) }
func (r *terminalRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.text, r.cursor}
}
func (r *terminalRenderer) Destroy() {}

func (r *terminalRenderer) Refresh() {
	r.background.FillColor = resolveShellColor(terminalBackground)
	r.background.Refresh()
	r.cursor.FillColor = terminalCursorColor()
	e := r.terminal.emulator
	cellSize := r.terminal.cellSize()
	used := 0
	for y := 0; y < e.Height(); y++ {
		for x := 0; x < e.Width(); {
			start := x
			var style uv.Style
			if cell := r.terminal.cellAt(x, y); cell != nil {
				style = cell.Style
			}
			var value strings.Builder
			for x < e.Width() {
				cell := r.terminal.cellAt(x, y)
				if cell == nil {
					if !style.IsZero() {
						break
					}
					value.WriteByte(' ')
					x++
					continue
				}
				if !style.Equal(&cell.Style) {
					break
				}
				if cell.Content == "" {
					value.WriteByte(' ')
				} else {
					value.WriteString(cell.Content)
				}
				x += max(1, cell.Width)
			}
			if used == len(r.pool) {
				r.pool = append(r.pool, canvas.NewText("", resolveShellColor(terminalForeground)))
				r.backgrounds = append(r.backgrounds, canvas.NewRectangle(resolveShellColor(terminalBackground)))
			}
			text := r.pool[used]
			text.Text, text.TextSize = value.String(), r.terminal.textSize
			text.TextStyle = fyne.TextStyle{Monospace: true, Bold: style.Attrs&uv.AttrBold != 0, Italic: style.Attrs&uv.AttrItalic != 0}
			text.Color = style.Fg
			if text.Color == nil {
				text.Color = resolveShellColor(terminalForeground)
			}
			bg := style.Bg
			if bg == nil {
				bg = resolveShellColor(terminalBackground)
			}
			if style.Attrs&uv.AttrReverse != 0 {
				text.Color, bg = bg, text.Color
			}
			text.Color = terminalInkColor(text.Color, bg)
			r.backgrounds[used].FillColor = bg
			r.backgrounds[used].Move(fyne.NewPos(float32(start)*cellSize.Width, float32(y)*cellSize.Height))
			r.backgrounds[used].Resize(fyne.NewSize(float32(x-start)*cellSize.Width, cellSize.Height))
			r.backgrounds[used].Refresh()
			text.Move(fyne.NewPos(float32(start)*cellSize.Width, float32(y)*cellSize.Height))
			text.Show()
			text.Refresh()
			used++
		}
	}
	objects := make([]fyne.CanvasObject, used*2)
	for i := range r.pool {
		if i < used {
			objects[i*2], objects[i*2+1] = r.backgrounds[i], r.pool[i]
		} else {
			r.pool[i].Hide()
		}
	}
	r.text.Objects = objects
	position := e.CursorPosition()
	r.cursor.Move(fyne.NewPos(float32(position.X)*cellSize.Width, float32(position.Y)*cellSize.Height))
	r.cursor.Resize(cellSize)
	if r.terminal.focused && !r.terminal.closed && r.terminal.visibleCursor && r.terminal.scroll == 0 {
		r.cursor.Show()
	} else {
		r.cursor.Hide()
	}
	r.text.Refresh()
	r.cursor.Refresh()
}

func terminalCursorColor() color.Color {
	shade := color.NRGBAModel.Convert(resolveShellColor(terminalForeground)).(color.NRGBA)
	shade.A = 128
	return shade
}

func (t *terminalSurface) cellAt(x, y int) *uv.Cell {
	if t.scroll == 0 {
		return t.emulator.CellAt(x, y)
	}
	index := t.emulator.ScrollbackLen() - t.scroll + y
	if index < t.emulator.ScrollbackLen() {
		return t.emulator.ScrollbackCellAt(x, index)
	}
	return t.emulator.CellAt(x, index-t.emulator.ScrollbackLen())
}
func (t *terminalSurface) Scrolled(event *fyne.ScrollEvent) {
	if t.altScreen || t.closed {
		return
	}
	if event.Scrolled.DY > 0 {
		t.scroll = min(t.emulator.ScrollbackLen(), t.scroll+3)
	} else {
		t.scroll = max(0, t.scroll-3)
	}
	t.Refresh()
}
