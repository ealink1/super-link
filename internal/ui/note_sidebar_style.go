package ui

import (
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
)

var noteSidebarSurface = sharedUIColor(theme.ColorNameBackground)
var noteSidebarSelection = sharedUIColor(theme.ColorNameSelection)
var noteSidebarAccent = sharedUIColor(theme.ColorNamePrimary)
var noteSidebarInk = sharedUIColor(theme.ColorNameForeground)

type noteSidebarTheme struct{ shellTheme }

func (t noteSidebarTheme) Color(name fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	switch name {
	case theme.ColorNamePrimary:
		return t.shellTheme.Color(theme.ColorNamePrimary, v)
	case theme.ColorNameSelection:
		return t.shellTheme.Color(theme.ColorNameSelection, v)
	case theme.ColorNameInputBackground:
		return resolveShellColor(shellTone{day: color.NRGBA{245, 245, 247, 255}, night: color.NRGBA{38, 41, 50, 255}})
	case theme.ColorNameBackground:
		return t.shellTheme.Color(theme.ColorNameBackground, v)
	case theme.ColorNameForeground:
		return t.shellTheme.Color(theme.ColorNameForeground, v)
	}
	return t.shellTheme.Color(name, v)
}

type noteRowTheme struct {
	shellLabelTheme
	row   *noteRow
	title bool
}

func (t noteRowTheme) Color(name fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		if t.title {

			return resolveShellColor(noteSidebarInk)
		}
		return resolveShellColor(shellMutedColor)
	}
	return t.shellLabelTheme.Color(name, v)
}
func noteSidebarSummary(body string) string {
	value := []rune(body)
	if len(value) > 1024 {
		value = value[:1024]
	}
	var result strings.Builder
	for _, line := range notePresentation(string(value)) {
		for _, run := range line.runs {
			result.WriteString(run.text)
			result.WriteByte(' ')
		}
	}
	return strings.Join(strings.Fields(result.String()), " ")
}

func (t noteSidebarTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInputRadius, theme.SizeNameSelectionRadius:
		return 10
	case theme.SizeNameText:
		return 14
	}
	return t.shellTheme.Size(name)
}

type noteButtonTheme struct{ noteSidebarTheme }

func (t noteButtonTheme) Font(style fyne.TextStyle) fyne.Resource {
	style.Bold = false
	return t.shellTheme.Font(style)
}
func noteButtonView(button *shellAlignedButton) fyne.CanvasObject {
	return container.NewThemeOverride(button, noteButtonTheme{noteSidebarTheme{newShellTheme()}})
}

// The notebook keeps folder hierarchy but uses flat, full-width note rows.
type noteTreeTheme struct{ noteSidebarTheme }

func (t noteTreeTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameInlineIcon || name == theme.SizeNamePadding {
		return 0
	}
	return t.noteSidebarTheme.Size(name)
}
