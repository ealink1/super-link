package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"github.com/ealink1/super-link/internal/application"
)

func (p *shellPane) buildSessionContent() fyne.CanvasObject {
	p.indicator = newConnectionDot()
	p.indicator.status = application.ConnectionConnecting
	p.remoteActions = [2]*shellAlignedButton{
		shellButton("SFTP", "folder", false, p.showFiles),
		shellButton("监控", "monitor", false, p.showMonitor),
	}
	for _, button := range p.remoteActions {
		button.Disable()
	}
	tools := shellHBox(shellButtonView(p.remoteActions[0]), shellFixed(layout.NewSpacer(), 8, 0), shellButtonView(p.remoteActions[1]), shellFixed(layout.NewSpacer(), 8, 0), shellButtonView(shellButton("重连", "refresh-cw", false, p.reconnect)))
	font := shellHBox(shellButtonView(shellButton("−", "", false, func() { p.adjustFont(-1) })), shellText("字号", 11, false, shellMutedColor), shellButtonView(shellButton("+", "", false, func() { p.adjustFont(1) })))
	status := shellBorder(nil, nil, shellHBox(shellFixed(p.indicator, 14, 14), shellFixed(layout.NewSpacer(), 6, 0)), nil, shellLabel(p.status, 11))
	footer := shellVBox(shellLine(), shellFixed(shellInset(shellBorder(nil, nil, tools, font, shellInset(status, 6)), 6), 0, 32))
	p.aux.Hide()
	return shellBorder(nil, footer, nil, p.aux, shellInset(p.terminal, 8))
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
	p.auxKind = ""
	p.aux.Hide()
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
	p.aux.Objects = []fyne.CanvasObject{container.New(&shellAuxiliaryLayout{canvas: p.workspace.owner.Window.Canvas()}, panel)}
	p.auxKind = kind
	p.aux.Show()
	p.aux.Refresh()
	p.workspace.content.Refresh()
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
