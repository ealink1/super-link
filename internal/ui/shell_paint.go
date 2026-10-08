package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
)

// Resolve semantic colors into concrete colors before reaching the painter.
type shellPrimitive struct {
	widget.BaseWidget
	object       fyne.CanvasObject
	fill, stroke color.Color
	textOffset   float32
}

func shellRectangle(fill color.Color, radius float32, stroke color.Color) *shellPrimitive {
	rectangle := canvas.NewRectangle(resolveShellColor(fill))
	rectangle.CornerRadius = radius
	if stroke != nil {
		rectangle.StrokeWidth = 1
		rectangle.StrokeColor = resolveShellColor(stroke)
	}
	p := &shellPrimitive{object: rectangle, fill: fill, stroke: stroke}
	p.ExtendBaseWidget(p)
	return p
}

func shellText(text string, size float32, bold bool, shade color.Color) fyne.CanvasObject {
	label := canvas.NewText(text, resolveShellColor(shade))
	label.TextSize, label.TextStyle.Bold = size, bold
	p := &shellPrimitive{object: label, fill: shade, textOffset: 2}
	p.ExtendBaseWidget(p)
	return p
}

func (p *shellPrimitive) CreateRenderer() fyne.WidgetRenderer {
	return &shellPrimitiveRenderer{primitive: p}
}

type shellPrimitiveRenderer struct{ primitive *shellPrimitive }

func (r *shellPrimitiveRenderer) MinSize() fyne.Size { return r.primitive.object.MinSize() }
func (r *shellPrimitiveRenderer) Layout(size fyne.Size) {
	r.primitive.object.Resize(size)
	if _, ok := r.primitive.object.(*canvas.Text); ok {
		r.primitive.object.Move(fyne.NewPos(0, -r.primitive.textOffset))
	}
}
func (r *shellPrimitiveRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.primitive.object}
}
func (*shellPrimitiveRenderer) Destroy() {}
func (r *shellPrimitiveRenderer) Refresh() {
	p := r.primitive
	switch object := p.object.(type) {
	case *canvas.Rectangle:
		object.FillColor = resolveShellColor(p.fill)
		if p.stroke != nil {
			object.StrokeColor = resolveShellColor(p.stroke)
		}
	case *canvas.Text:
		object.Color = resolveShellColor(p.fill)
	}
	p.object.Refresh()
}
