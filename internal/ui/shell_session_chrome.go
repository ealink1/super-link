package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/application"
)

func (p *shellPane) buildSessionContent() fyne.CanvasObject {
	p.indicator = newConnectionDot()
	p.indicator.status = application.ConnectionConnecting
	p.remoteActions = [1]*shellAlignedButton{
		shellButton("SFTP", "folder", false, p.showFiles),
	}
	for _, button := range p.remoteActions {
		button.Disable()
	}
	tools := shellHBox(shellButtonView(p.remoteActions[0]), shellFixed(layout.NewSpacer(), 8, 0), shellButtonView(shellButton("重连", "refresh-cw", false, p.reconnect)))
	font := shellHBox(shellButtonView(shellButton("−", "", false, func() { p.adjustFont(-1) })), shellText("字号", 11, false, shellMutedColor), shellButtonView(shellButton("+", "", false, func() { p.adjustFont(1) })))
	status := container.New(&shellSessionStatusLayout{}, p.indicator, shellLabel(p.status, 12))
	footer := shellVBox(shellLine(), shellFixed(shellInset(shellBorder(nil, nil, tools, font, status), 6), 0, 32))
	p.aux.Hide()
	p.auxExpand = shellButton("", "panel-left-close", false, p.restoreAuxiliary)
	p.auxHandle = shellPanel(container.NewThemeOverride(p.auxExpand, shellAuxiliaryHandleTheme{shellButtonTheme: shellButtonTheme{shellTheme: newShellTheme()}}), monitorSurfaceColor, 6, 0)
	p.auxHandle.Hide()
	body := container.New(&shellAuxiliaryHandleLayout{}, shellBorder(nil, nil, nil, p.aux, shellInset(p.terminal, 8)), p.auxHandle)
	return shellBorder(nil, footer, nil, nil, body)
}

// Match the button text's optical center without squeezing the status label
// through a second vertical inset in the compact session toolbar.
type shellSessionStatusLayout struct{}

func (*shellSessionStatusLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[1].MinSize().Add(fyne.NewSize(28, 0))
}
func (*shellSessionStatusLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Resize(fyne.NewSize(14, 14))
	objects[0].Move(fyne.NewPos(8, (size.Height-14)/2))
	height := objects[1].MinSize().Height
	objects[1].Resize(fyne.NewSize(max(0, size.Width-28), height))
	objects[1].Move(fyne.NewPos(28, (size.Height-height)/2-2))
}

func (p *shellPane) adjustFont(delta float32) {
	p.terminal.setTextSize(min(24, max(10, p.terminal.textSize+delta)))
	p.workspace.owner.Window.Canvas().Focus(p.terminal)
}

func (p *shellPane) setSessionState(text string, ended bool) {
	p.status.SetText(text)
	p.indicator.status = application.ConnectionConnected
	if ended {
		p.indicator.status = application.ConnectionFailed
	}
	p.indicator.Refresh()
	if p.workspace.page == 1 {
		p.workspace.showTerminals()
	}
}

func (p *shellPane) hideAuxiliary() {
	if p.auxKind != "" {
		p.collapsedAuxKind = p.auxKind
	}
	p.auxKind = ""
	p.aux.Hide()
	if p.collapsedAuxKind != "" {
		p.auxHandle.Show()
	}
	p.aux.Refresh()
	p.workspace.content.Refresh()
}

func (p *shellPane) restoreAuxiliary() {
	if p.closed || p.collapsedAuxKind == "" || len(p.aux.Objects) == 0 {
		return
	}
	p.auxKind = p.collapsedAuxKind
	p.collapsedAuxKind = ""
	p.auxHandle.Hide()
	p.aux.Show()
	p.aux.Refresh()
	p.workspace.content.Refresh()
}

func (p *shellPane) showAuxiliary(kind, title string, body fyne.CanvasObject) {
	if p.aux.Visible() && p.auxKind == kind {
		p.hideAuxiliary()
		return
	}
	header := shellFixed(shellInset(shellBorder(nil, nil, shellText(title, 12, true, shellTextColor), shellButtonView(shellButton("", "panel-left-close", false, p.hideAuxiliary)), layout.NewSpacer()), 10), 0, 40)
	panel := container.NewStack(shellRectangle(shellPanelColor, 0, nil), shellBorder(shellVBox(header, shellLine()), nil, nil, nil, body))
	if kind == "monitor" {
		panel = container.NewStack(body)
	}
	p.aux.Objects = []fyne.CanvasObject{container.New(&shellAuxiliaryLayout{canvas: p.workspace.owner.Window.Canvas()}, panel)}
	p.auxKind = kind
	p.collapsedAuxKind = ""
	p.auxHandle.Hide()
	p.aux.Show()
	p.aux.Refresh()
	p.workspace.content.Refresh()
}

// The collapsed handle overlays the terminal without reserving sidebar space.
type shellAuxiliaryHandleLayout struct{}

func (*shellAuxiliaryHandleLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[0].MinSize()
}
func (*shellAuxiliaryHandleLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Move(fyne.Position{})
	objects[0].Resize(size)
	width, height := min(float32(24), size.Width), min(float32(64), size.Height)
	objects[1].Resize(fyne.NewSize(width, height))
	objects[1].Move(fyne.NewPos(size.Width-width, (size.Height-height)/2))
}

type shellAuxiliaryHandleTheme struct{ shellButtonTheme }

func (t shellAuxiliaryHandleTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInlineIcon:
		return 12
	case theme.SizeNameInnerPadding:
		return 4
	}
	return t.shellButtonTheme.Size(name)
}

// Side panels fill the terminal height rather than leaving a fixed 560px island.
// Keep enough terminal columns in a small desktop window.
type shellAuxiliaryLayout struct{ canvas fyne.Canvas }

func (l *shellAuxiliaryLayout) MinSize([]fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(min(420, max(320, l.canvas.Size().Width*0.29)), 200)
}
func (*shellAuxiliaryLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Move(fyne.Position{})
	objects[0].Resize(size)
}
