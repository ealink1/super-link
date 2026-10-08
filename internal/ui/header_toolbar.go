package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

func newHeaderAction(label string, run func()) fyne.CanvasObject {
	var resource fyne.Resource
	switch label {
	case "新建查询":
		resource = theme.DocumentCreateIcon()
	case "新建连接":
		resource = icon("link")
	case "管理连接分组":
		resource = headerOutlineIcon("groups", `<path d="M4 3v14a2 2 0 0 0 2 2h3M4 7h5M4 14h5"/><path d="M10 4h4l2 2h4v5H10zM10 14h4l2 2h4v5H10z"/>`)
	case "SQL 工具":
		resource = headerOutlineIcon("tools", `<path d="M14 6a5 5 0 0 0-6 6L2 18a2.8 2.8 0 0 0 4 4l6-6a5 5 0 0 0 6-6l-4 4-4-4 4-4z"/>`)
	case "驱动管理":
		resource = icon("package")
	default:
		resource = theme.InfoIcon()
	}
	primary := label == "新建查询"
	image := widget.NewIcon(resource)
	caption := widget.NewLabel(label)
	caption.Alignment = fyne.TextAlignCenter
	content := container.New(&headerActionLayout{}, image, caption)
	visual := container.NewThemeOverride(content, headerCaptionTheme{headerButtonTheme{primary: primary}})
	button := action(label, "", run)
	hit := container.NewThemeOverride(button, headerHitTheme{})
	return container.NewStack(visual, hit)
}

type headerCaptionTheme struct{ headerButtonTheme }

func (t headerCaptionTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 12
	}
	return fyne.CurrentApp().Settings().Theme().Size(name)
}

type headerHitTheme struct{ Theme }

func (t headerHitTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground || name == theme.ColorNameButton {
		return color.Transparent
	}
	return fyne.CurrentApp().Settings().Theme().Color(name, variant)
}

type headerActionLayout struct{}

func (*headerActionLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(max(64, objects[1].MinSize().Width), 52)
}
func (*headerActionLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Resize(fyne.NewSize(24, 24))
	objects[0].Move(fyne.NewPos((size.Width-24)/2, 11))
	objects[1].Resize(fyne.NewSize(size.Width, 24))
	objects[1].Move(fyne.NewPos(0, 26))
}

func headerOutlineIcon(name, geometry string) fyne.Resource {
	return &outlineIcon{name: name, source: []byte(`<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">` + geometry + `</svg>`)}
}
