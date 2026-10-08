package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// Keep standard button activation and accessibility, while owning the complete
// renderer. Delegating to Button's renderer temporarily restores its different
// icon/label layout during hover refreshes on the native canvas.
type monitorTabButton struct {
	widget.Button
	pointerInside, keyboardFocused bool
}

func newMonitorTabButton(name string, icon fyne.Resource, action func()) *monitorTabButton {
	b := &monitorTabButton{}
	b.Text, b.Icon, b.OnTapped, b.Importance = name, icon, action, widget.LowImportance
	b.ExtendBaseWidget(b)
	return b
}
func (b *monitorTabButton) MouseIn(*desktop.MouseEvent)    { b.pointerInside = true; b.Refresh() }
func (b *monitorTabButton) MouseMoved(*desktop.MouseEvent) {}
func (b *monitorTabButton) MouseOut()                      { b.pointerInside = false; b.Refresh() }
func (b *monitorTabButton) FocusGained()                   { b.keyboardFocused = true; b.Button.FocusGained() }
func (b *monitorTabButton) FocusLost()                     { b.keyboardFocused = false; b.Button.FocusLost() }
func (b *monitorTabButton) CreateRenderer() fyne.WidgetRenderer {
	background := canvas.NewRectangle(color.Transparent)
	background.CornerRadius = 4
	icon := canvas.NewImageFromResource(b.Icon)
	icon.FillMode = canvas.ImageFillContain
	label := widget.NewLabel(b.Text)
	return &monitorTabButtonRenderer{button: b, background: background, icon: icon, label: label}
}

type monitorTabButtonRenderer struct {
	button     *monitorTabButton
	background *canvas.Rectangle
	icon       *canvas.Image
	label      *widget.Label
}

func (r *monitorTabButtonRenderer) MinSize() fyne.Size {
	return fyne.NewSize(30+r.label.MinSize().Width, 28)
}
func (r *monitorTabButtonRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	textSize := r.label.MinSize()
	const iconSize, gap = float32(14), float32(4)
	x := (size.Width - iconSize - gap - textSize.Width) / 2
	r.icon.Move(fyne.NewPos(x, (size.Height-iconSize)/2))
	r.icon.Resize(fyne.NewSquareSize(iconSize))
	textY := (size.Height - textSize.Height) / 2
	// Bundled CJK glyphs have a different line-box baseline from Latin labels.
	if r.button.Text == "综合" {
		textY -= 3
	}
	r.label.Move(fyne.NewPos(x+iconSize+gap, textY))
	r.label.Resize(textSize)
}
func (r *monitorTabButtonRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.icon, r.label}
}
func (*monitorTabButtonRenderer) Destroy() {}
func (r *monitorTabButtonRenderer) Refresh() {
	fill := color.Color(color.Transparent)
	if !r.button.Disabled() {
		variant := fyne.CurrentApp().Settings().ThemeVariant()
		if r.button.pointerInside {
			fill = r.button.Theme().Color(theme.ColorNameHover, variant)
		}
		if r.button.keyboardFocused {
			fill = r.button.Theme().Color(theme.ColorNameFocus, variant)
		}
	}
	r.background.FillColor = fill
	r.background.Refresh()
	r.icon.Resource = r.button.Icon
	r.icon.Refresh()
	if r.label.Text != r.button.Text {
		r.label.SetText(r.button.Text)
	} else {
		r.label.Refresh()
	}
	r.Layout(r.button.Size())
}
