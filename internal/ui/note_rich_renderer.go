package ui

import (
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
)

type noteCaretPoint struct {
	offset   int
	position fyne.Position
	height   float32
}
type noteRichRenderer struct {
	entry   *noteEntry
	base    fyne.WidgetRenderer
	objects []fyne.CanvasObject
	points  []noteCaretPoint
	height  float32
	width   float32
}

func (r *noteRichRenderer) Destroy() { r.base.Destroy() }
func (r *noteRichRenderer) Objects() []fyne.CanvasObject {
	if !r.entry.rich {
		return r.base.Objects()
	}
	return r.objects
}
func (r *noteRichRenderer) MinSize() fyne.Size {
	if !r.entry.rich {
		return r.base.MinSize()
	}
	return fyne.NewSize(200, max(160, r.height))
}
func (r *noteRichRenderer) Layout(size fyne.Size) {
	r.base.Layout(size)
	r.width = size.Width
	r.draw()
}
func (r *noteRichRenderer) Refresh() { r.base.Refresh(); r.draw(); canvas.Refresh(r.entry) }
func (r *noteRichRenderer) draw() {
	if !r.entry.rich {
		return
	}
	r.objects = nil
	r.points = nil
	foreground := r.entry.Theme().Color(theme.ColorNameForeground, fyne.CurrentApp().Settings().ThemeVariant())
	if r.entry.Disabled() {
		foreground = r.entry.Theme().Color(theme.ColorNameDisabled, fyne.CurrentApp().Settings().ThemeVariant())
	}
	y := float32(12)
	for _, line := range notePresentation(r.entry.Text) {
		if line.fence {
			continue
		}
		x := float32(0)
		indent := float32(0)
		if line.marker != "" || line.quote {
			indent = 26
			x = indent
		}
		height := line.size * 1.6
		var codeBackground *canvas.Rectangle
		codeY := y
		if line.code {
			indent = 12
			x = indent
			codeBackground = canvas.NewRectangle(r.entry.Theme().Color(theme.ColorNameHover, fyne.CurrentApp().Settings().ThemeVariant()))
			codeBackground.Move(fyne.NewPos(0, y-6))
			r.objects = append(r.objects, codeBackground)
		}
		if line.marker != "" {
			r.addText(line.marker, fyne.NewPos(0, y), 16, fyne.TextStyle{}, foreground)
		}
		if line.quote {
			bar := canvas.NewRectangle(r.entry.Theme().Color(theme.ColorNameSeparator, fyne.CurrentApp().Settings().ThemeVariant()))
			bar.Move(fyne.NewPos(0, y))
			bar.Resize(fyne.NewSize(3, height))
			r.objects = append(r.objects, bar)
		}
		start := line.start
		if len(line.runs) > 0 {
			start = line.runs[0].start
		}
		r.points = append(r.points, noteCaretPoint{start, fyne.NewPos(x, y), height})
		for _, run := range line.runs {
			chunk := ""
			chunkPosition := fyne.NewPos(x, y)
			flush := func() {
				if chunk != "" {
					if run.style.Monospace && !line.code {
						background := canvas.NewRectangle(r.entry.Theme().Color(theme.ColorNameHover, fyne.CurrentApp().Settings().ThemeVariant()))
						background.Move(chunkPosition)
						background.Resize(fyne.MeasureText(chunk, run.size, run.style))
						r.objects = append(r.objects, background)
					}
					r.addText(chunk, chunkPosition, run.size, run.style, foreground)
					chunk = ""
				}
			}
			for i, ch := range []rune(run.text) {
				value := string(ch)
				w := fyne.MeasureText(value, run.size, run.style).Width
				if x+w > max(40, r.width) && x > indent {
					flush()
					x = indent
					y += height
				}
				r.points = append(r.points, noteCaretPoint{run.start + i, fyne.NewPos(x, y), height})
				if chunk == "" {
					chunkPosition = fyne.NewPos(x, y)
				}
				chunk += value
				x += w
				r.points = append(r.points, noteCaretPoint{run.start + i + 1, fyne.NewPos(x, y), height})
			}
			flush()
		}
		r.points = append(r.points, noteCaretPoint{line.end, fyne.NewPos(x, y), height})
		if codeBackground != nil {
			codeBackground.Resize(fyne.NewSize(r.width, y-codeY+height+12))
		}
		y += height + 12
	}
	r.height = y + 16
	r.drawSelection()
	if r.entry.Text == "" {
		r.addText("开始记录…", fyne.NewPos(0, 12), 16, fyne.TextStyle{}, r.entry.Theme().Color(theme.ColorNamePlaceHolder, fyne.CurrentApp().Settings().ThemeVariant()))
	}
	if r.entry.richFocused && !r.entry.Disabled() {
		offset := r.entry.sourceOffset()
		point := r.nearestOffset(offset)
		caret := canvas.NewRectangle(r.entry.Theme().Color(theme.ColorNamePrimary, fyne.CurrentApp().Settings().ThemeVariant()))
		caret.Move(point.position)
		caret.Resize(fyne.NewSize(2, point.height))
		r.objects = append(r.objects, caret)
	}
}
func (r *noteRichRenderer) drawSelection() {
	selected := len([]rune(r.entry.SelectedText()))
	if selected > 0 {
		end := r.entry.sourceOffset()
		start := end - selected
		if r.entry.selectionAnchor > end {
			start = end
			end += selected
		}
		var highlights []fyne.CanvasObject
		col := r.entry.Theme().Color(theme.ColorNameSelection, fyne.CurrentApp().Settings().ThemeVariant())
		for i, p := range r.points {
			if p.offset < start || p.offset >= end || i+1 >= len(r.points) {
				continue
			}
			next := r.points[i+1]
			if next.position.Y != p.position.Y || next.position.X <= p.position.X {
				continue
			}
			rect := canvas.NewRectangle(col)
			rect.Move(p.position)
			rect.Resize(fyne.NewSize(next.position.X-p.position.X, p.height))
			highlights = append(highlights, rect)
		}
		r.objects = append(highlights, r.objects...)
	}
}
func (r *noteRichRenderer) addText(value string, pos fyne.Position, size float32, style fyne.TextStyle, col color.Color) {
	text := canvas.NewText(value, col)
	text.TextSize = size
	text.TextStyle = style
	text.Move(pos)
	text.Resize(text.MinSize())
	r.objects = append(r.objects, text)
}
func (r *noteRichRenderer) nearestOffset(offset int) noteCaretPoint {
	best := noteCaretPoint{position: fyne.NewPos(0, 12), height: 26}
	distance := int(^uint(0) >> 1)
	for _, p := range r.points {
		d := p.offset - offset
		if d < 0 {
			d = -d
		}
		if d <= distance {
			best = p
			distance = d
		}
	}
	return best
}
func (e *noteEntry) sourceOffset() int {
	lines := strings.Split(e.Text, "\n")
	offset := 0
	for i := 0; i < e.CursorRow && i < len(lines); i++ {
		offset += len([]rune(lines[i])) + 1
	}
	return offset + e.CursorColumn
}
func (e *noteEntry) setSourceOffset(offset int) {
	e.CursorRow = 0
	e.CursorColumn = 0
	for i, ch := range []rune(e.Text) {
		if i >= offset {
			break
		}
		if ch == '\n' {
			e.CursorRow++
			e.CursorColumn = 0
		} else {
			e.CursorColumn++
		}
	}
}
func (e *noteEntry) Tapped(event *fyne.PointEvent) {
	if e.richRenderer == nil || !e.rich {
		e.Entry.Tapped(event)
		return
	}
	if e.Disabled() {
		return
	}
	fyne.CurrentApp().Driver().CanvasForObject(e).Focus(e)

	e.Refresh()
}
func (e *noteEntry) FocusGained()           { e.richFocused = true; e.Entry.FocusGained(); e.Refresh() }
func (e *noteEntry) FocusLost()             { e.richFocused = false; e.Entry.FocusLost(); e.Refresh() }
func (e *noteEntry) Cursor() desktop.Cursor { return desktop.TextCursor }
