package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

func toolbarAction(label, name string, run func()) fyne.CanvasObject {
	button := action(label, name, run)
	return framedControl(button)
}

func framedControl(control fyne.CanvasObject) fyne.CanvasObject {
	frame := &controlFrame{control: control}
	frame.ExtendBaseWidget(frame)
	return frame
}

type controlFrame struct {
	widget.BaseWidget
	control fyne.CanvasObject
}

func (f *controlFrame) CreateRenderer() fyne.WidgetRenderer {
	background := canvas.NewRectangle(theme.ButtonColor())
	background.CornerRadius = 8
	background.StrokeWidth = 0.5
	background.StrokeColor = theme.InputBorderColor()
	return &controlFrameRenderer{frame: f, background: background}
}

type controlFrameRenderer struct {
	frame      *controlFrame
	background *canvas.Rectangle
}

func (r *controlFrameRenderer) MinSize() fyne.Size {
	size := r.frame.control.MinSize()
	return fyne.NewSize(max(32, size.Width), max(32, size.Height))
}
func (r *controlFrameRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.frame.control.Resize(size)
}
func (r *controlFrameRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.frame.control}
}
func (r *controlFrameRenderer) Refresh() {
	r.background.FillColor = theme.ButtonColor()
	r.background.StrokeColor = theme.InputBorderColor()
	r.background.Refresh()
	r.frame.control.Refresh()
}
func (r *controlFrameRenderer) Destroy() {}

func headerAction(label string, run func()) fyne.CanvasObject {
	return newHeaderAction(label, run)
}

func headerDivider() fyne.CanvasObject {
	line := widget.NewSeparator()
	return container.New(layout.NewCustomPaddedLayout(12, 12, 12, 12), line)
}

type headerButtonTheme struct {
	Theme
	primary bool
}

func (t headerButtonTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	current := fyne.CurrentApp().Settings().Theme()
	if name == theme.ColorNameForeground && t.primary {
		return color.NRGBA{R: 51, G: 122, B: 255, A: 255}
	}
	return current.Color(name, variant)
}
