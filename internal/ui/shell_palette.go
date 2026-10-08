package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
)

// Immutable color roles are resolved by widget renderers on every refresh.
// They never hold a Window or page, and are never used directly as texture keys.
type shellColor fyne.ThemeColorName

func (c shellColor) RGBA() (r, g, b, a uint32) { return theme.Color(fyne.ThemeColorName(c)).RGBA() }

const (
	shellBackground  = shellColor(theme.ColorNameBackground)
	shellPanelColor  = shellColor(theme.ColorNameOverlayBackground)
	shellRailColor   = shellColor(theme.ColorNameButton)
	shellBorderColor = shellColor(theme.ColorNameInputBorder)
	shellTextColor   = shellColor(theme.ColorNameForeground)
	shellMutedColor  = shellColor(theme.ColorNamePlaceHolder)
	shellAccentColor = shellColor(theme.ColorNamePrimary)
)

type shellTone struct{ day, night color.NRGBA }

func (c shellTone) RGBA() (r, g, b, a uint32) {
	if currentAppearanceDark() {
		return c.night.RGBA()
	}
	return c.day.RGBA()
}

func currentAppearanceDark() bool {
	if app := fyne.CurrentApp(); app != nil {
		if selected, ok := app.Settings().Theme().(interface{ appearancePalette() *appearancePalette }); ok {
			palette := selected.appearancePalette()
			return palette != nil && palette.dark.Load()
		}
	}
	return false
}

func resolveShellColor(shade color.Color) color.Color {
	return color.NRGBAModel.Convert(shade)
}

// Resolve shared roles directly, bypassing local widget color overrides.
// A themed widget must not query its own foreground through theme.Color.
type sharedUIColor fyne.ThemeColorName

func (c sharedUIColor) RGBA() (uint32, uint32, uint32, uint32) {
	return (Theme{Dark: currentAppearanceDark()}).Color(fyne.ThemeColorName(c), theme.VariantLight).RGBA()
}
