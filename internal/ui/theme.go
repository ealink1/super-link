package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
)

type Theme struct {
	Dark    bool
	palette *appearancePalette
}

func (t Theme) appearancePalette() *appearancePalette {
	if t.palette != nil {
		return t.palette
	}
	palette := &appearancePalette{}
	palette.dark.Store(t.Dark)
	return palette
}

func (t Theme) Font(style fyne.TextStyle) fyne.Resource {
	return desktopFonts.font(style)
}
func (t Theme) Icon(name fyne.ThemeIconName) fyne.Resource {
	if name == theme.IconNameNavigateNext || name == theme.IconNameMoveDown {
		path := "M9 5 L16 12 L9 19 Z"
		if name == theme.IconNameMoveDown {
			path = "M5 9 L12 16 L19 9 Z"
		}
		fill := "#6b7280"
		if t.Dark {
			fill = "#9ca3af"
		}
		return fyne.NewStaticResource(string(name)+"-"+fill+".svg", []byte(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="`+fill+`" d="`+path+`"/></svg>`))
	}
	return theme.DefaultTheme().Icon(name)
}
func (t Theme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameText:
		return 13
	case theme.SizeNamePadding:
		return 4
	case theme.SizeNameInnerPadding:
		return 6
	case theme.SizeNameInlineIcon:
		return 16
	case theme.SizeNameInputRadius, theme.SizeNameSelectionRadius:
		return 6
	case theme.SizeNameSeparatorThickness:
		return 0.5
	case theme.SizeNameInputBorder:
		return 0.5
	case theme.SizeNameSplitThickness:
		return 1
	case theme.SizeNameScrollBar:
		return 6
	}
	return theme.DefaultTheme().Size(name)
}
func (t Theme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if t.Dark {
		variant = theme.VariantDark
	} else {
		variant = theme.VariantLight
	}
	switch name {
	case theme.ColorNameDisabled:
		return color.NRGBA{R: 156, G: 163, B: 175, A: 255}
	case theme.ColorNamePrimary:
		if t.Dark {
			return color.NRGBA{R: 34, G: 197, B: 94, A: 255}
		}
		return color.NRGBA{R: 21, G: 128, B: 61, A: 255}
	case theme.ColorNameSelection:
		if t.Dark {
			return color.NRGBA{R: 24, G: 63, B: 44, A: 255}
		}
		return color.NRGBA{R: 200, G: 219, B: 203, A: 255}
	case theme.ColorNameBackground:
		if t.Dark {
			return color.NRGBA{R: 27, G: 31, B: 39, A: 255}
		}
		if !t.Dark {
			return color.NRGBA{R: 250, G: 250, B: 248, A: 255}
		}
	case theme.ColorNameInputBackground:
		if t.Dark {
			return color.NRGBA{R: 15, G: 18, B: 23, A: 255}
		}
		if !t.Dark {
			return color.White
		}
	case theme.ColorNameButton:
		if t.Dark {
			return color.NRGBA{R: 22, G: 26, B: 33, A: 255}
		}
		if !t.Dark {
			return color.NRGBA{R: 247, G: 248, B: 246, A: 255}
		}
	case theme.ColorNameForeground:
		if t.Dark {
			return color.NRGBA{R: 245, G: 245, B: 245, A: 255}
		}
		if !t.Dark {
			return color.NRGBA{R: 31, G: 41, B: 55, A: 255}
		}
	case theme.ColorNameInputBorder, theme.ColorNameSeparator:
		if t.Dark {
			return color.NRGBA{R: 255, G: 255, B: 255, A: 25}
		}
		if !t.Dark {
			return color.NRGBA{R: 15, G: 23, B: 42, A: 28}
		}
	}
	return theme.DefaultTheme().Color(name, variant)
}
