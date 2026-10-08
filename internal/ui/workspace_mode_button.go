package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

type workspaceModeButton struct {
	widget.Button
	selected bool
}

func newWorkspaceModeButton(text string, run func()) *workspaceModeButton {
	b := &workspaceModeButton{}
	b.ExtendBaseWidget(b)
	b.Text, b.OnTapped, b.Importance = text, run, widget.LowImportance
	return b
}
func (b *workspaceModeButton) CreateRenderer() fyne.WidgetRenderer {
	return &workspaceModeRenderer{button: b, base: b.Button.CreateRenderer(), line: canvas.NewRectangle(color.Transparent)}
}

type workspaceModeRenderer struct {
	button *workspaceModeButton
	base   fyne.WidgetRenderer
	line   *canvas.Rectangle
}

func (r *workspaceModeRenderer) MinSize() fyne.Size { return r.base.MinSize() }
func (r *workspaceModeRenderer) Layout(size fyne.Size) {
	r.base.Layout(size)
	alignShellButtonText(r.base, size)
	width := min(fyne.MeasureText(r.button.Text, 14, fyne.TextStyle{}).Width, size.Width)
	r.line.Move(fyne.NewPos((size.Width-width)/2, size.Height-2))
	r.line.Resize(fyne.NewSize(width, 2))
}
func (r *workspaceModeRenderer) Objects() []fyne.CanvasObject {
	return append(r.base.Objects(), r.line)
}
func (r *workspaceModeRenderer) Destroy() { r.base.Destroy() }
func (r *workspaceModeRenderer) Refresh() {
	r.base.Refresh()
	r.line.FillColor = color.Transparent
	if r.button.selected {
		r.line.FillColor = color.NRGBA{R: 51, G: 122, B: 255, A: 255}
	}
	r.line.Refresh()
	r.Layout(r.button.Size())
}
