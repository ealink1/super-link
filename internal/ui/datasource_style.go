package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"image/color"
	"math"
)

type sourcePickerTheme struct{ Theme }

func (t sourcePickerTheme) shade(name string) color.Color {
	light := map[string]string{"background": "#f5f6f8", "panel": "#ffffff", "line": "#e6e8ec", "text": "#1f2329", "muted": "#8f959e", "brand": "#2f6bff", "soft": "#eaf1ff", "tag": "#f2f4f7", "hover": "#c9d6ff"}
	dark := map[string]string{"background": "#171b23", "panel": "#212630", "line": "#363e4b", "text": "#eef1f6", "muted": "#a2acbb", "brand": "#83aaff", "soft": "#273c61", "tag": "#2b3340", "hover": "#668ee0"}
	if t.Dark {
		return hexColor(dark[name])
	}
	return hexColor(light[name])
}
func (t sourcePickerTheme) Color(name fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	switch name {
	case theme.ColorNamePrimary, theme.ColorNameFocus:
		return t.shade("brand")
	case theme.ColorNameSelection:
		return t.shade("soft")
	case theme.ColorNameInputBackground, theme.ColorNameBackground:
		return t.shade("panel")
	case theme.ColorNameInputBorder, theme.ColorNameSeparator:
		return t.shade("line")
	case theme.ColorNameForeground:
		return t.shade("text")
	case theme.ColorNameDisabled, theme.ColorNamePlaceHolder:
		return t.shade("muted")
	case theme.ColorNameHover:
		return t.shade("tag")
	}
	return t.Theme.Color(name, v)
}
func (t sourcePickerTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInputRadius, theme.SizeNameSelectionRadius:
		return 10
	case theme.SizeNameInputBorder:
		return 1
	case theme.SizeNameInnerPadding:
		return 10
	}
	return t.Theme.Size(name)
}

type sourceColumnsLayout struct{}

func (*sourceColumnsLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(500, 300) }
func (*sourceColumnsLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	side := float32(232)
	if size.Width < 850 {
		side = 190
	}
	objects[0].Move(fyne.NewPos(0, 0))
	objects[0].Resize(fyne.NewSize(side, size.Height))
	objects[1].Move(fyne.NewPos(side+16, 0))
	objects[1].Resize(fyne.NewSize(max(0, size.Width-side-16), size.Height))
}

type sourceGridLayout struct{ columns int }

func (l *sourceGridLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	rows := int(math.Ceil(float64(len(objects)) / float64(max(1, l.columns))))
	return fyne.NewSize(240, max(0, float32(rows)*116-12))
}
func (l *sourceGridLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	l.columns = max(1, int((size.Width+12)/272))
	width := (size.Width - float32(l.columns-1)*12) / float32(l.columns)
	for i, obj := range objects {
		obj.Move(fyne.NewPos(float32(i%l.columns)*(width+12), float32(i/l.columns)*116))
		obj.Resize(fyne.NewSize(width, 104))
	}
}
