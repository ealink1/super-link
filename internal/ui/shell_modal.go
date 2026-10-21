package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

type shellModal struct {
	popup   *widget.PopUp
	scope   *container.ThemeOverride
	onClose func()
}

type shellModalTheme struct{ shellTheme }

type shellFormTheme struct{ shellTheme }

func (t shellFormTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 12
	}
	return t.shellTheme.Size(name)
}

func (t shellModalTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameShadow {
		return color.NRGBA{A: 180}
	}
	return t.shellTheme.Color(name, variant)
}

func (s *shellWorkspace) newModal(title string, body, footer fyne.CanvasObject, close func()) *shellModal {
	m := &shellModal{onClose: close}
	header := shellInset(shellBorder(nil, nil, shellText(title, 14, true, shellTextColor), shellButtonView(shellButton("", "x", false, m.hide)), layout.NewSpacer()), 11)
	content := shellPanel(shellBorder(shellVBox(header, shellLine()), shellVBox(shellLine(), shellInset(footer, 16)), nil, nil, container.NewVScroll(shellInset(body, 24))), shellBackground, cardRadius, 0)
	m.popup = widget.NewModalPopUp(container.NewThemeOverride(content, shellFormTheme{shellTheme: newShellTheme()}), s.owner.Window.Canvas())
	m.scope = container.NewThemeOverride(m.popup, shellModalTheme{shellTheme: newShellTheme()})
	m.popup.Resize(fyne.NewSize(min(672, s.owner.Window.Canvas().Size().Width-48), min(776, s.owner.Window.Canvas().Size().Height-48)))
	return m
}

func (m *shellModal) hide() {
	m.popup.Hide()
	if m.onClose != nil {
		m.onClose()
	}
}

func shellField(title string, input fyne.CanvasObject) fyne.CanvasObject {
	return shellVBox(shellText(title, 12, false, shellTextColor), shellFixed(layout.NewSpacer(), 0, 6), shellFixed(input, 0, 32))
}

func shellSection(title string) fyne.CanvasObject {
	return shellVBox(shellFixed(layout.NewSpacer(), 0, 16), shellText(title, 12, true, shellMutedColor), shellFixed(layout.NewSpacer(), 0, 12))
}
