package ui

import (
	"image"
	"image/color"
	"math"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// Paint a rounded gradient directly on the native canvas, including the
// transparent corners that Fyne's SVG rectangle painter does not preserve.
func groupGradient(start, end string, radius float32) fyne.CanvasObject {
	from, to := color.NRGBAModel.Convert(hexColor(start)).(color.NRGBA), color.NRGBAModel.Convert(hexColor(end)).(color.NRGBA)
	return canvas.NewRaster(func(width, height int) image.Image {
		result := image.NewNRGBA(image.Rect(0, 0, width, height))
		w, h := float64(width), float64(height)
		r := float64(radius) / 42 * min(w, h)
		for y := 0; y < height; y++ {
			for x := 0; x < width; x++ {
				qx, qy := math.Abs(float64(x)+.5-w/2)-(w/2-r), math.Abs(float64(y)+.5-h/2)-(h/2-r)
				distance := math.Hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r
				coverage := min(1, max(0, .5-distance))
				fraction := (float64(x)/max(1, w-1) + float64(y)/max(1, h-1)) / 2
				mix := func(a, b uint8) uint8 { return uint8(float64(a)*(1-fraction) + float64(b)*fraction) }
				result.SetNRGBA(x, y, color.NRGBA{R: mix(from.R, to.R), G: mix(from.G, to.G), B: mix(from.B, to.B), A: uint8(255 * coverage)})
			}
		}
		return result
	})
}

func groupFormIcon(name string) fyne.Resource {
	resource := shellIcon(name, false)
	if outline, ok := resource.(*shellOutlineIcon); ok {
		outline.shade = theme.ColorNameForegroundOnPrimary
	}
	if outline, ok := resource.(*outlineIcon); ok {
		outline.shade = "#ffffff"
	}
	return resource
}

type connectionGroupSubmitButton struct{ shellAlignedButton }

func newConnectionGroupSubmitButton(run func()) *connectionGroupSubmitButton {
	b := &connectionGroupSubmitButton{}
	b.Text, b.Importance, b.OnTapped = "创建分组", widget.HighImportance, run
	b.ExtendBaseWidget(b)
	return b
}

func (b *connectionGroupSubmitButton) CreateRenderer() fyne.WidgetRenderer {
	return &connectionGroupSubmitRenderer{content: &shellAlignedButtonRenderer{button: &b.shellAlignedButton, content: b.Button.CreateRenderer()}, background: groupGradient("#4f6ef7", "#6b84fa", 11.5)}
}

type connectionGroupSubmitRenderer struct {
	content    fyne.WidgetRenderer
	background fyne.CanvasObject
}

func (r *connectionGroupSubmitRenderer) MinSize() fyne.Size { return r.content.MinSize() }
func (r *connectionGroupSubmitRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.content.Layout(size)
}
func (r *connectionGroupSubmitRenderer) Objects() []fyne.CanvasObject {
	return append([]fyne.CanvasObject{r.background}, r.content.Objects()...)
}
func (r *connectionGroupSubmitRenderer) Destroy() { r.content.Destroy() }
func (r *connectionGroupSubmitRenderer) Refresh() { r.background.Refresh(); r.content.Refresh() }

type connectionGroupSwatch struct {
	widget.Button
	active, focused bool
	background      fyne.CanvasObject
	symbol          fyne.CanvasObject
}

func newConnectionGroupSwatch(start, end, text, icon string) *connectionGroupSwatch {
	s := &connectionGroupSwatch{background: groupGradient(start, end, 11)}
	if icon != "" {
		s.symbol = shellFixed(widget.NewIcon(groupFormIcon(icon)), 18, 18)
	} else {
		s.symbol = shellText(text, 15, true, color.White)
	}
	s.ExtendBaseWidget(s)
	return s
}
func (s *connectionGroupSwatch) FocusGained() { s.focused = true; s.Refresh() }
func (s *connectionGroupSwatch) FocusLost()   { s.focused = false; s.Refresh() }
func (s *connectionGroupSwatch) CreateRenderer() fyne.WidgetRenderer {
	outer := canvas.NewRectangle(color.Transparent)
	outer.CornerRadius, outer.StrokeWidth = 14, 1.5
	inner := canvas.NewRectangle(color.Transparent)
	inner.CornerRadius, inner.StrokeWidth, inner.StrokeColor = 11, 2, color.White
	content := container.NewStack(s.background, inner, container.NewCenter(s.symbol))
	r := &connectionGroupSwatchRenderer{swatch: s, outer: outer, content: content}
	r.Refresh()
	return r
}

type connectionGroupSwatchRenderer struct {
	swatch  *connectionGroupSwatch
	outer   *canvas.Rectangle
	content *fyne.Container
}

func (*connectionGroupSwatchRenderer) MinSize() fyne.Size { return fyne.NewSize(48, 48) }
func (r *connectionGroupSwatchRenderer) Layout(size fyne.Size) {
	r.outer.Move(fyne.NewPos(1, 1))
	r.outer.Resize(size.Subtract(fyne.NewSize(2, 2)))
	r.content.Move(fyne.NewPos(3, 3))
	r.content.Resize(size.Subtract(fyne.NewSize(6, 6)))
}
func (r *connectionGroupSwatchRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.outer, r.content}
}
func (r *connectionGroupSwatchRenderer) Destroy() {}
func (r *connectionGroupSwatchRenderer) Refresh() {
	r.outer.StrokeColor = color.Transparent
	if r.swatch.active {
		r.outer.StrokeColor = theme.Color(theme.ColorNameForeground)
	}
	if r.swatch.focused {
		r.outer.StrokeColor = connectionGroupPrimary
	}
	r.outer.Refresh()
	r.Layout(r.swatch.Size())
}

type connectionGroupToggle struct {
	widget.Check
	focused bool
}

func newConnectionGroupToggle() *connectionGroupToggle {
	t := &connectionGroupToggle{}
	t.ExtendBaseWidget(t)
	return t
}
func (t *connectionGroupToggle) Tapped(*fyne.PointEvent) {
	if !t.Disabled() {
		t.SetChecked(!t.Checked)
	}
}
func (t *connectionGroupToggle) FocusGained() { t.focused = true; t.Refresh() }
func (t *connectionGroupToggle) FocusLost()   { t.focused = false; t.Refresh() }
func (t *connectionGroupToggle) TypedRune(r rune) {
	if r == ' ' {
		t.Tapped(nil)
	}
}
func (t *connectionGroupToggle) CreateRenderer() fyne.WidgetRenderer {
	track := canvas.NewRectangle(hexColor("#dcdfe6"))
	track.CornerRadius = 12
	knob := canvas.NewCircle(color.White)
	r := &connectionGroupToggleRenderer{toggle: t, track: track, knob: knob}
	r.Refresh()
	return r
}

type connectionGroupToggleRenderer struct {
	toggle *connectionGroupToggle
	track  *canvas.Rectangle
	knob   *canvas.Circle
}

func (*connectionGroupToggleRenderer) MinSize() fyne.Size { return fyne.NewSize(42, 24) }
func (r *connectionGroupToggleRenderer) Layout(size fyne.Size) {
	r.track.Resize(size)
	r.knob.Resize(fyne.NewSquareSize(size.Height - 4))
	x := float32(2)
	if r.toggle.Checked {
		x = size.Width - size.Height + 2
	}
	r.knob.Move(fyne.NewPos(x, 2))
}
func (r *connectionGroupToggleRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.track, r.knob}
}
func (r *connectionGroupToggleRenderer) Destroy() {}
func (r *connectionGroupToggleRenderer) Refresh() {
	r.track.FillColor = hexColor("#dcdfe6")
	if r.toggle.Checked {
		r.track.FillColor = connectionGroupPrimary
	}
	r.track.StrokeWidth = 0
	if r.toggle.focused {
		r.track.StrokeWidth, r.track.StrokeColor = 2, theme.Color(theme.ColorNameFocus)
	}
	if r.toggle.Disabled() {
		r.track.FillColor = theme.Color(theme.ColorNameDisabled)
	}
	r.track.Refresh()
	r.Layout(r.toggle.Size())
}
