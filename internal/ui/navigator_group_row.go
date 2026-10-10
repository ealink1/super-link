package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

const (
	navigatorCountHeight   = 16
	navigatorCountMinWidth = 18
	navigatorCountPadding  = 5
	navigatorCountGap      = 2 // the pill carries its own padding, so the boxes may touch
	// Measured on both CJK and Latin names: this font's glyphs sit ~2pt below the row's geometric centre.
	navigatorCountDrop = 2
)

// The count sits beside a coloured folder icon, so it stays neutral and recedes into the rail.
var (
	navigatorCountFill  = shellTone{day: color.NRGBA{R: 236, G: 236, B: 232, A: 255}, night: color.NRGBA{R: 44, G: 49, B: 58, A: 255}}
	navigatorCountShade = shellTone{day: color.NRGBA{R: 107, G: 114, B: 128, A: 255}, night: color.NRGBA{R: 156, G: 163, B: 175, A: 255}}
)

type navigatorCountTheme struct{ shellTheme }

func (t navigatorCountTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		return resolveShellColor(navigatorCountShade)
	}
	return t.shellTheme.Color(name, variant)
}
func (t navigatorCountTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameText:
		return 11
	case theme.SizeNameInnerPadding:
		return 0
	}
	return t.shellTheme.Size(name)
}

func newNavigatorGroupCount(label *widget.Label) fyne.CanvasObject {
	content := container.NewThemeOverride(label, navigatorCountTheme{shellTheme: newShellTheme()})
	return container.New(&navigatorCountLayout{}, shellRectangle(navigatorCountFill, navigatorCountHeight/2, nil), content)
}

type navigatorCountLayout struct{}

func (l *navigatorCountLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(max(navigatorCountMinWidth, objects[1].MinSize().Width+2*navigatorCountPadding), navigatorCountHeight)
}
func (l *navigatorCountLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	background, content := objects[0], objects[1]
	background.Move(fyne.Position{})
	background.Resize(size)
	natural := content.MinSize()
	content.Move(fyne.NewPos((size.Width-natural.Width)/2, (size.Height-natural.Height)/2))
	content.Resize(natural)
}

type navigatorGroupLabelLayout struct{ row *treeRow }

func (l *navigatorGroupLabelLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[0].MinSize()
}
func (l *navigatorGroupLabelLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	label, badge := objects[0], objects[1]
	width := size.Width
	if badge.Visible() {
		badgeSize := badge.MinSize()
		width = min(max(0, size.Width-badgeSize.Width-navigatorCountGap), fyne.MeasureText(l.row.label.Text, theme.TextSize(), l.row.label.TextStyle).Width+2*theme.InnerPadding())
		badge.Resize(badgeSize)
		badge.Move(fyne.NewPos(width+navigatorCountGap-theme.InnerPadding(), (size.Height-badgeSize.Height)/2+navigatorCountDrop))
	}
	label.Move(fyne.Position{})
	label.Resize(fyne.NewSize(width, size.Height))
}
