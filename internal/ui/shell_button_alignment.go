package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
)

// Keep Fyne button interaction while compensating for the desktop font's
// line-box whitespace relative to the adjacent icon.
type shellAlignedButton struct {
	widget.Button
	bold       bool
	textOffset float32
}

func (b *shellAlignedButton) CreateRenderer() fyne.WidgetRenderer {
	return &shellAlignedButtonRenderer{button: b, content: b.Button.CreateRenderer()}
}

type shellAlignedButtonRenderer struct {
	button  *shellAlignedButton
	content fyne.WidgetRenderer
}

func (r *shellAlignedButtonRenderer) MinSize() fyne.Size { return r.content.MinSize() }
func (r *shellAlignedButtonRenderer) Layout(size fyne.Size) {
	r.content.Layout(size)
	alignShellButtonText(r.content, size)
	if r.button.textOffset != 0 {
		for _, object := range r.content.Objects() {
			if _, ok := object.(*widget.RichText); ok {
				object.Move(object.Position().Add(fyne.NewPos(0, r.button.textOffset)))
			}
		}
	}
}
func (r *shellAlignedButtonRenderer) Objects() []fyne.CanvasObject { return r.content.Objects() }
func (r *shellAlignedButtonRenderer) Destroy()                     { r.content.Destroy() }
func (r *shellAlignedButtonRenderer) Refresh() {
	r.content.Refresh()
	r.Layout(r.button.Size())
}
func alignShellButtonText(renderer fyne.WidgetRenderer, size fyne.Size) {
	for _, object := range renderer.Objects() {
		if _, ok := object.(*widget.RichText); ok {
			height := object.MinSize().Height
			object.Resize(fyne.NewSize(object.Size().Width, height))
			object.Move(fyne.NewPos(object.Position().X, (size.Height-height)/2-2))
		}
	}
}
