package ui

import (
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

var documentSelectedGreen = color.NRGBA{R: 0x20, G: 0xD9, B: 0x62, A: 0xFF}

type documentTab struct {
	widget.BaseWidget
	title, subtitle     string
	selected            bool
	selectTab, closeTab func()
	tooltip             *documentTooltip
}

func newDocumentTab(title, subtitle string, selected bool, selectTab, closeTab func()) *documentTab {
	t := &documentTab{title: title, subtitle: subtitle, selected: selected, selectTab: selectTab, closeTab: closeTab}
	t.ExtendBaseWidget(t)
	return t
}

func (t *documentTab) Tapped(*fyne.PointEvent) {
	t.hideTooltip()
	if t.selectTab != nil {
		t.selectTab()
	}
}
func (t *documentTab) close() {
	t.hideTooltip()
	if t.closeTab != nil {
		t.closeTab()
	}
}
func (t *documentTab) CreateRenderer() fyne.WidgetRenderer {
	background := canvas.NewRectangle(theme.BackgroundColor())
	background.CornerRadius = 8
	background.StrokeColor = theme.InputBorderColor()
	background.StrokeWidth = 0.5
	indicator := canvas.NewCircle(documentSelectedGreen)
	if !t.selected {
		indicator.Hide()
	}
	title := canvas.NewText(t.title, theme.ForegroundColor())
	title.TextStyle = fyne.TextStyle{Bold: true}
	title.TextSize = 13
	close := widget.NewButtonWithIcon("", theme.CancelIcon(), t.close)
	close.Importance = widget.LowImportance
	return &documentTabRenderer{t: t, background: background, indicator: indicator, title: title, close: close}
}

type documentTabRenderer struct {
	t          *documentTab
	background *canvas.Rectangle
	indicator  *canvas.Circle
	title      *canvas.Text
	close      *widget.Button
}

func (r *documentTabRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.indicator.Move(fyne.NewPos(9, (size.Height-8)/2))
	r.indicator.Resize(fyne.NewSize(8, 8))
	left := float32(9)
	if r.t.selected {
		left = 23
	}
	width := fyne.Max(0, size.Width-left-29)
	r.title.Alignment = fyne.TextAlignLeading
	r.title.Text = fitText(r.t.title, width, r.title.TextSize, r.title.TextStyle)
	height := r.title.MinSize().Height
	// The desktop font has extra space above its visible glyphs. Offset the
	// line box slightly upward so the text aligns optically with the dot.
	r.title.Move(fyne.NewPos(left, (size.Height-height)/2-2))
	r.title.Resize(fyne.NewSize(width, height))
	r.close.Move(fyne.NewPos(size.Width-30, (size.Height-26)/2))
	r.close.Resize(fyne.NewSize(26, 26))
}
func (r *documentTabRenderer) MinSize() fyne.Size { return fyne.NewSize(132, 32) }
func (r *documentTabRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.indicator, r.title, r.close}
}
func (r *documentTabRenderer) Refresh() {
	r.background.FillColor = theme.BackgroundColor()
	r.background.StrokeColor = theme.InputBorderColor()
	if r.t.selected {
		r.indicator.Show()
	} else {
		r.indicator.Hide()
	}
	r.indicator.FillColor = documentSelectedGreen
	r.title.Color = theme.ForegroundColor()
	r.Layout(r.t.Size())
	for _, object := range r.Objects() {
		object.Refresh()
	}
	canvas.Refresh(r.t)
}
func (r *documentTabRenderer) Destroy() { r.t.hideTooltip() }

func fitText(value string, width, size float32, style fyne.TextStyle) string {
	if fyne.MeasureText(value, size, style).Width <= width {
		return value
	}
	runes := []rune(value)
	for len(runes) > 0 && fyne.MeasureText(string(runes)+"…", size, style).Width > width {
		runes = runes[:len(runes)-1]
	}
	return strings.TrimSpace(string(runes)) + "…"
}
