package ui

import (
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

var _ desktop.Hoverable = (*documentTab)(nil)

// One passive layer is shared by all headers. Fyne pop-ups capture mouse events
// for the entire canvas, so they cannot serve as a tooltip over clickable tabs.
type documentTooltip struct {
	layer, box *fyne.Container
	label      *widget.Label
	background *canvas.Rectangle
	active     fyne.CanvasObject
}

func newDocumentTooltip() *documentTooltip {
	label := widget.NewLabel("")
	label.Wrapping = fyne.TextWrapWord
	background := canvas.NewRectangle(theme.OverlayBackgroundColor())
	background.CornerRadius = 5
	background.StrokeColor = theme.InputBorderColor()
	background.StrokeWidth = 0.5
	box := container.NewStack(background, container.NewPadded(label))
	box.Hide()
	return &documentTooltip{layer: container.New(&documentTooltipLayout{}, box), box: box, label: label, background: background}
}

func (t *documentTab) MouseIn(*desktop.MouseEvent) {
	if t.tooltip != nil {
		t.tooltip.show(t)
	}
}

func (t *documentTab) MouseMoved(*desktop.MouseEvent) {}

func (t *documentTab) MouseOut() { t.hideTooltip() }

func (t *documentTab) hideTooltip() {
	if t.tooltip != nil && t.tooltip.active == t {
		t.tooltip.hide()
	}
}

func (h *documentTooltip) show(t *documentTab) {
	text := t.title
	if t.subtitle != "" {
		text += "\n" + t.subtitle
	}
	h.showContent(t, text)
}

func (h *documentTooltip) showContent(t fyne.CanvasObject, text string) {
	h.showContentAt(t, text, nil)
}

func (h *documentTooltip) showContentAt(t fyne.CanvasObject, text string, event *desktop.MouseEvent) {
	driver := fyne.CurrentApp().Driver()
	parent := driver.CanvasForObject(t)
	if parent == nil || len(parent.Overlays().List()) != 0 || h.layer.Size().IsZero() {
		return
	}
	h.active = t
	h.label.SetText(text)
	h.background.FillColor = theme.OverlayBackgroundColor()
	h.background.StrokeColor = theme.InputBorderColor()
	width := float32(0)
	for _, line := range strings.Split(text, "\n") {
		width = max(width, fyne.MeasureText(line, theme.TextSize(), fyne.TextStyle{}).Width)
	}
	// Include both the outer container padding and Label's inner padding.
	width = min(width+2*theme.InnerPadding()+2*theme.Padding()+2, 360, h.layer.Size().Width)
	h.label.Resize(fyne.NewSize(max(0, width-2*theme.Padding()), 0))
	size := fyne.NewSize(width, h.label.MinSize().Height+2*theme.Padding())
	h.box.Resize(size)
	position := driver.AbsolutePositionForObject(t).Subtract(driver.AbsolutePositionForObject(h.layer))
	position.Y += t.Size().Height + 4
	if event != nil {
		position = event.AbsolutePosition.Subtract(driver.AbsolutePositionForObject(h.layer))
		position.Y += 16
	}
	position.X = max(0, min(position.X, h.layer.Size().Width-size.Width))
	position.Y = max(0, min(position.Y, h.layer.Size().Height-size.Height))
	h.box.Move(position)
	h.box.Show()
	h.box.Refresh()
}

func (h *documentTooltip) moveBelowPointer(event *desktop.MouseEvent) {
	if event == nil || h.active == nil || !h.box.Visible() {
		return
	}
	origin := fyne.CurrentApp().Driver().AbsolutePositionForObject(h.layer)
	position := event.AbsolutePosition.Subtract(origin)
	position.Y += 16
	size := h.box.Size()
	position.X = max(0, min(position.X, h.layer.Size().Width-size.Width))
	position.Y = max(0, min(position.Y, h.layer.Size().Height-size.Height))
	h.box.Move(position)
}

func (h *documentTooltip) hide() {
	h.box.Hide()
	h.label.SetText("")
	h.active = nil
}

// Tooltips must not contribute their position or width to the window minimum.
type documentTooltipLayout struct{}

func (*documentTooltipLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.Size{} }
func (*documentTooltipLayout) Layout([]fyne.CanvasObject, fyne.Size) {}
