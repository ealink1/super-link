package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"image/color"
)

// Read-only DDL should remain as legible as enabled text.
type ddlTextTheme struct{ fyne.Theme }

func (t ddlTextTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameDisabled {
		name = theme.ColorNameForeground
	}
	return t.Theme.Color(name, variant)
}
func (t ddlTextTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 14
	}
	return t.Theme.Size(name)
}
