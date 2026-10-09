package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// Standard Fyne buttons retain focus, keyboard activation and disabled semantics.
func shellButton(text, iconName string, primary bool, run func()) *shellAlignedButton {
	aligned := &shellAlignedButton{}
	aligned.ExtendBaseWidget(aligned)
	button := &aligned.Button
	button.Text, button.OnTapped = text, run
	if iconName != "" {
		resource := shellIcon(iconName, false)
		if outline, ok := resource.(*shellOutlineIcon); ok && primary {
			outline.shade = theme.ColorNameForegroundOnPrimary
		}
		button.SetIcon(resource)
	}
	if primary {
		button.Importance = widget.HighImportance
	} else {
		button.Importance = widget.LowImportance
	}
	return aligned
}

func shellOutlined(button *shellAlignedButton) fyne.CanvasObject {
	if button.Importance == widget.LowImportance {
		button.Importance = widget.MediumImportance
	}
	return shellPanel(shellButtonView(button), color.Transparent, 4, 0)
}

type shellButtonTheme struct {
	shellTheme
	bold bool
}

func (t shellButtonTheme) Font(style fyne.TextStyle) fyne.Resource {
	style.Bold = t.bold
	return t.shellTheme.Font(style)
}
func (t shellButtonTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 12
	}
	return t.shellTheme.Size(name)
}

func shellButtonView(button *shellAlignedButton) fyne.CanvasObject {
	return container.NewThemeOverride(button, shellButtonTheme{shellTheme: newShellTheme(), bold: button.bold})
}

func shellTinted(button *shellAlignedButton) fyne.CanvasObject {
	button.Importance = widget.LowImportance
	panel := shellPanel(shellButtonView(button), shellColor(theme.ColorNameSelection), 4, 0)
	panel.Objects[0].(*shellPrimitive).stroke = shellAccentColor
	return panel
}

type shellNavButton struct {
	widget.Button
	name       string
	selected   bool
	background *canvas.Rectangle

	tooltip string
	owner   *shellWorkspace
}

func newShellNav(s *shellWorkspace, name, label string, run func()) *shellNavButton {
	b := &shellNavButton{name: name, tooltip: label, owner: s}
	b.OnTapped, b.Importance = run, widget.LowImportance
	b.Icon = shellIcon(name, false)
	b.ExtendBaseWidget(b)
	return b
}

func (b *shellNavButton) CreateRenderer() fyne.WidgetRenderer {
	b.background = canvas.NewRectangle(color.Transparent)
	b.background.CornerRadius = 12
	return &shellNavRenderer{button: b, content: b.Button.CreateRenderer()}
}

func (b *shellNavButton) MouseIn(event *desktop.MouseEvent) {
	b.Button.MouseIn(event)
	b.owner.showNavHint(b.tooltip)
}
func (b *shellNavButton) MouseOut() {
	b.Button.MouseOut()
	b.owner.showNavHint("")
}

type shellNavRenderer struct {
	button  *shellNavButton
	content fyne.WidgetRenderer
}

func (*shellNavRenderer) MinSize() fyne.Size { return fyne.NewSize(40, 40) }
func (r *shellNavRenderer) Layout(size fyne.Size) {
	r.button.background.Resize(size)
	r.content.Layout(size)
	alignShellButtonText(r.content, size)
}
func (r *shellNavRenderer) Objects() []fyne.CanvasObject {
	return append([]fyne.CanvasObject{r.button.background}, r.content.Objects()...)
}
func (r *shellNavRenderer) Destroy() { r.content.Destroy() }
func (r *shellNavRenderer) Refresh() {
	fill := color.Color(color.Transparent)
	if r.button.selected {
		fill = theme.Color(theme.ColorNameSelection)
	}
	r.button.background.FillColor = fill
	r.button.background.Refresh()
	r.button.Icon = shellIcon(r.button.name, r.button.selected)
	r.content.Refresh()
	r.Layout(r.button.Size())
}

func shellBadge(text string) fyne.CanvasObject {
	return shellPanel(shellInset(shellText(text, 11, false, shellMutedColor), 4), color.Transparent, 4, 0)
}

func shellLine() fyne.CanvasObject {
	return shellFixed(shellRectangle(shellBorderColor, 0, nil), 0, 1)
}

func shellImage(name string, active bool, size float32) fyne.CanvasObject {
	image := canvas.NewImageFromResource(shellIcon(name, active))
	image.FillMode = canvas.ImageFillContain
	return container.NewGridWrap(fyne.NewSquareSize(size), image)
}

type shellLabelTheme struct {
	shellTheme
	size float32
}

func (t shellLabelTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameInnerPadding {
		return 0
	}
	if name == theme.SizeNameText && t.size > 0 {
		return t.size
	}
	return t.shellTheme.Size(name)
}

func shellLabel(label *widget.Label, size float32) fyne.CanvasObject {
	return container.NewThemeOverride(label, shellLabelTheme{shellTheme: newShellTheme(), size: size})
}

func shellColumns(gap float32, objects ...fyne.CanvasObject) *fyne.Container {
	return container.New(&shellColumnsLayout{gap: gap}, objects...)
}

type shellColumnsLayout struct {
	gap   float32
	slots int
}

func (l *shellColumnsLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	var size fyne.Size
	count := 0
	for _, obj := range objects {
		if obj.Visible() {
			count++
			size.Width = max(size.Width, obj.MinSize().Width)
			size.Height = max(size.Height, obj.MinSize().Height)
		}
	}
	size.Width = size.Width*float32(count) + l.gap*float32(max(0, count-1))
	return size
}
func (l *shellColumnsLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	visible := []fyne.CanvasObject{}
	for _, obj := range objects {
		if obj.Visible() {
			visible = append(visible, obj)
		}
	}
	if len(visible) == 0 {
		return
	}
	count := max(len(visible), l.slots)
	width := max(0, (size.Width-l.gap*float32(count-1))/float32(count))
	for i, obj := range visible {
		obj.Move(fyne.NewPos(float32(i)*(width+l.gap), 0))
		obj.Resize(fyne.NewSize(width, size.Height))
	}
}
