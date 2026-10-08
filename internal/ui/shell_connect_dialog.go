package ui

import (
	"context"
	"errors"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellConnectDialog struct {
	pane   *shellPane
	model  *shellConnectModel
	popup  *widget.PopUp
	scope  *container.ThemeOverride
	steps  [4]*shellConnectStep
	lines  [3]*shellPrimitive
	logs   *fyne.Container
	scroll *container.Scroll
	title  *widget.Label
	hint   *widget.Label
	busy   *widget.Activity
	action *shellAlignedButton
	retry  *shellAlignedButton
	ended  bool
	hidden bool
}

type shellConnectTextTheme struct {
	shellTheme
	shade color.Color
}

type shellConnectActionTheme struct{ shellButtonTheme }

func (t shellConnectActionTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 14
	}
	if name == theme.SizeNameInputRadius {
		return 9
	}
	return t.shellButtonTheme.Size(name)
}

func (t shellConnectActionTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNamePrimary {
		return resolveShellColor(shellConnectPink)
	}
	return t.shellButtonTheme.Color(name, variant)
}

func (t shellConnectTextTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		return resolveShellColor(t.shade)
	}
	return t.shellTheme.Color(name, variant)
}

func newShellConnectDialog(p *shellPane) *shellConnectDialog {
	d := &shellConnectDialog{pane: p, model: newShellConnectModel(), logs: shellVBox()}
	d.title, d.hint = widget.NewLabel("SSH 连接中…"), widget.NewLabel("连接日志仅显示阶段说明")
	d.title.TextStyle.Bold = true
	d.hint.Wrapping = fyne.TextWrapWord
	d.action = shellButton("取消连接", "", true, d.dismiss)
	d.retry = shellButton("重新连接", "refresh-cw", true, func() { d.close(); p.reconnect() })
	d.retry.Hide()
	d.busy = widget.NewActivity()
	steps, stageViews, lines := newShellConnectSteps()
	d.steps, d.lines = stageViews, lines
	activity := container.NewThemeOverride(d.busy, shellConnectTextTheme{shellTheme: newShellTheme(), shade: shellConnectBlue})
	badge := shellFixed(shellPanel(shellInset(activity, 7), shellRailColor, 17, 0), 34, 34)
	header := shellInset(shellBorder(nil, nil, shellHBox(badge, shellFixed(layout.NewSpacer(), 12, 0), shellLabel(d.title, 15)), shellButtonView(shellButton("", "x", false, d.dismiss)), layout.NewSpacer()), 16)
	top := shellVBox(header, steps, shellFixed(layout.NewSpacer(), 0, 6), shellLine())
	d.scroll = container.NewVScroll(shellInset(d.logs, 12))
	logHeader := shellInset(shellHBox(shellImage("terminal", false, 16), shellFixed(layout.NewSpacer(), 8, 0), shellText("连接日志", 12, true, shellMutedColor)), 12)
	logPanel := shellPanel(shellBorder(shellVBox(logHeader, shellLine()), nil, nil, nil, d.scroll), shellColor(theme.ColorNameInputBackground), 8, 0)
	action := container.NewThemeOverride(d.action, shellConnectActionTheme{shellButtonTheme{shellTheme: newShellTheme()}})
	footer := shellInset(shellBorder(nil, nil, shellLabel(d.hint, 11), shellHBox(shellButtonView(d.retry), shellFixed(layout.NewSpacer(), 8, 0), action), layout.NewSpacer()), 12)
	content := shellPanel(shellBorder(top, shellVBox(shellLine(), footer), nil, nil, shellInset(logPanel, 18)), shellBackground, 12, 0)
	d.popup = widget.NewModalPopUp(container.NewThemeOverride(content, newShellTheme()), p.workspace.owner.Window.Canvas())
	d.scope = container.NewThemeOverride(d.popup, shellModalTheme{shellTheme: newShellTheme()})
	size := p.workspace.owner.Window.Canvas().Size()
	d.popup.Resize(fyne.NewSize(min(512, max(352, size.Width-48)), min(400, max(320, size.Height-48))))
	d.model.record(transport.ConnectEvent{Stage: transport.ConnectTCP, State: transport.ConnectStarted})
	d.render()
	p.workspace.owner.jobs.watch(d.model.changes, func() {
		if !d.hidden && !d.ended {
			d.render()
		}
	})
	d.popup.Show()
	d.busy.Start()
	return d
}

func (d *shellConnectDialog) render() {
	events := d.model.snapshot()
	d.logs.Objects = nil
	for _, event := range events {
		step := d.steps[event.Stage]
		step.state = int(event.State)
		label := widget.NewLabel("[" + shellConnectTitles[event.Stage] + "]  " + shellConnectDescription(event))
		label.TextStyle.Monospace = true
		label.Wrapping = fyne.TextWrapWord
		d.logs.Add(container.NewThemeOverride(label, shellConnectTextTheme{shellTheme: newShellTheme(), shade: step.shade()}))
		if event.Stage < transport.ConnectReady && event.State == transport.ConnectCompleted {
			d.lines[event.Stage].fill = shellConnectBlue
			d.lines[event.Stage].Refresh()
		}
	}
	for _, step := range d.steps {
		step.Refresh()
	}
	d.scroll.Refresh()
	d.scroll.ScrollToBottom()
}

func (d *shellConnectDialog) finish(err error) {
	if d.ended {
		return
	}
	d.render()
	d.ended = true
	d.model.close()
	d.busy.Stop()
	d.busy.Hide()
	if err == nil {
		d.close()
		return
	}
	d.title.SetText("SSH 连接失败")
	d.hint.SetText("请检查主机、网络、认证方式或指纹")
	if errors.Is(err, context.Canceled) {
		d.title.SetText("连接已取消")
		d.hint.SetText("当前连接已停止")
	}
	d.action.SetText("关闭")
	d.action.Importance = widget.LowImportance
	d.retry.Show()
}

func (d *shellConnectDialog) dismiss() {
	if !d.ended {
		d.pane.finish(context.Canceled)
	}
	d.close()
}

func (d *shellConnectDialog) close() {
	d.hidden = true
	d.model.close()
	d.busy.Stop()
	d.popup.Hide()
}
