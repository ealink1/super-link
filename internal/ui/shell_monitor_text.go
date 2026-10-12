package ui

import (
	"image/color"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
)

const monitorTextPadding float32 = 2

// OpenGL rounds the text texture's width to a device pixel. Keep a blank cell
// after the visible text so rounding cannot cut the final glyph's ink.
const monitorTextRasterSuffix = " "

// Assign the font to the text primitive itself. A theme override alone paints
// SF Mono but measures the application's Courier font, clipping glyph textures.
type monitorMonoLabel struct {
	widget.BaseWidget
	text  string
	size  float32
	align fyne.TextAlign
	shade color.Color
}

func newMonitorMonoLabel(text string, size float32, align fyne.TextAlign, shade color.Color) *monitorMonoLabel {
	l := &monitorMonoLabel{text: text, size: size, align: align, shade: shade}
	l.ExtendBaseWidget(l)
	return l
}

func (l *monitorMonoLabel) AccessibilityLabel() string { return l.text }
func (*monitorMonoLabel) AccessibilityRole() fyne.AccessibleRole {
	return fyne.AccessibleRoleText
}

func (l *monitorMonoLabel) CreateRenderer() fyne.WidgetRenderer {
	text := canvas.NewText(l.text+monitorTextRasterSuffix, resolveShellColor(l.shade))
	text.TextSize = l.size
	text.TextStyle.Monospace = true
	text.FontSource = monitorMonoFont
	text.Alignment = l.align
	return &monitorMonoRenderer{label: l, text: text}
}

type monitorMonoRenderer struct {
	label *monitorMonoLabel
	text  *canvas.Text
}

func (r *monitorMonoRenderer) MinSize() fyne.Size {
	size := monitorTextSize("…"+monitorTextRasterSuffix, r.label.size)
	return size.Add(fyne.NewSquareSize(monitorTextPadding * 2))
}

func (r *monitorMonoRenderer) Layout(size fyne.Size) {
	width := max(0, size.Width-monitorTextPadding*2)
	text := monitorFitText(r.label.text, r.label.size, max(0, width-monitorTextSize(monitorTextRasterSuffix, r.label.size).Width))
	if text != "" {
		text += monitorTextRasterSuffix
	}
	changed := text != r.text.Text
	r.text.Text = text
	height := r.text.MinSize().Height
	r.text.Move(fyne.NewPos(monitorTextPadding, (size.Height-height)/2))
	r.text.Resize(fyne.NewSize(width, height))
	if changed {
		r.text.Refresh()
	}
}

func (r *monitorMonoRenderer) Refresh() {
	r.text.Color = resolveShellColor(r.label.shade)
	r.Layout(r.label.Size())
	r.text.Refresh()
}

func (r *monitorMonoRenderer) Objects() []fyne.CanvasObject { return []fyne.CanvasObject{r.text} }
func (*monitorMonoRenderer) Destroy()                       {}

func monitorTextSize(text string, size float32) fyne.Size {
	measured, _ := fyne.CurrentApp().Driver().RenderedTextSize(text, size, fyne.TextStyle{Monospace: true}, monitorMonoFont)
	return measured
}

func monitorFitText(text string, size, width float32) string {
	if monitorTextSize(text, size).Width <= width {
		return text
	}
	if monitorTextSize("…", size).Width > width {
		return ""
	}
	// Find the longest fitting UTF-8 prefix without allocating a rune slice.
	low, high := 0, len(text)
	for low < high {
		end := (low + high + 1) / 2
		for end < len(text) && !utf8.RuneStart(text[end]) {
			end++
		}
		if monitorTextSize(text[:end]+"…", size).Width <= width {
			low = end
		} else {
			high = end - 1
			for high > low && !utf8.RuneStart(text[high]) {
				high--
			}
		}
	}
	return text[:low] + "…"
}
