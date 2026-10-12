package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

func (s *shellWorkspace) buildTerminalPage() fyne.CanvasObject {
	s.terminalBody = container.NewStack()
	return shellBorder(s.terminalHeader(), nil, nil, nil, s.terminalBody)
}

func (s *shellWorkspace) terminalWelcome() fyne.CanvasObject {
	header := shellVBox(shellText("创建你的第一个终端会话", 14, true, shellTextColor), shellText("支持本地壳与 SSH，每个标签独立运行", 12, false, shellMutedColor))
	badges := shellHBox(shellBadge("本地与远程支持"), shellFixed(layout.NewSpacer(), 8, 0), shellBadge("多标签会话"))
	top := shellInset(shellBorder(nil, nil, header, badges, layout.NewSpacer()), 20)
	card := func(title, description string, buttons fyne.CanvasObject) fyne.CanvasObject {
		label := widget.NewLabel(description)
		label.Wrapping = fyne.TextWrapWord
		label.Importance = widget.LowImportance
		return shellPanel(shellVBox(shellText(title, 13, true, shellTextColor), shellFixed(layout.NewSpacer(), 0, 12), shellLabel(label, 12), shellFixed(layout.NewSpacer(), 0, 16), shellFixed(buttons, 0, 30)), shellBackground, 8, 16)
	}
	cards := shellColumns(16,
		card("本地终端（默认）", "使用系统默认壳，自动选择本机 Shell", shellHBox(shellFixed(shellTinted(shellButton("新建本地终端", "", false, func() { s.openLocal("") })), 100, 30), layout.NewSpacer())),
		card("选择本地壳", "从常用壳中选择并立即启动一个会话", shellHBox(shellOutlined(shellButton("/bin/zsh", "", false, func() { s.openLocal("/bin/zsh") })), shellFixed(layout.NewSpacer(), 8, 0), shellOutlined(shellButton("/bin/bash", "", false, func() { s.openLocal("/bin/bash") })))),
		card("远程 SSH 会话", "从主机列表中挑选目标，快速创建 SSH 会话", shellHBox(shellOutlined(shellButton("选择主机并创建会话", "", false, s.showHosts)), layout.NewSpacer())),
	)
	return shellPanel(shellVBox(top, shellLine(), shellInset(cards, 20)), shellPanelColor, 12, 0)
}

func (s *shellWorkspace) terminalHeader() fyne.CanvasObject {
	s.terminalButtons = make(map[*container.TabItem]*documentTab)
	s.terminalStrip = container.NewHBox()
	tabs := container.NewHScroll(container.NewThemeOverride(s.terminalStrip, Theme{}))
	tabs.SetMinSize(fyne.NewSize(0, 32))
	tools := shellHBox(shellButton("", "plus", false, s.quickConnect), shellFixed(layout.NewSpacer(), 8, 0), shellOutlined(shellButton("主机", "server", false, s.showHosts)))
	header := shellFixed(shellInset(shellBorder(nil, nil, nil, tools, tabs), 4), 0, 40)
	return shellVBox(header, shellLine())
}

func (s *shellWorkspace) selectedTerminalContent() fyne.CanvasObject {
	if item := s.tabs.Selected(); item != nil {
		return item.Content
	}
	return layout.NewSpacer()
}

func (s *shellWorkspace) quickConnect() {
	var popup *widget.PopUp
	button := func(label string, run func()) fyne.CanvasObject {
		return shellButton(label, "terminal", false, func() { popup.Hide(); run() })
	}
	menu := shellVBox(button("新建本地终端", func() { s.openLocal("") }), button("/bin/zsh", func() { s.openLocal("/bin/zsh") }), button("/bin/bash", func() { s.openLocal("/bin/bash") }), shellLine(), button("选择 SSH 主机", s.showHosts))
	popup = widget.NewPopUp(container.NewThemeOverride(shellInset(menu, 8), newShellTheme()), s.owner.Window.Canvas())
	popup.ShowAtPosition(fyne.NewPos(max(64, s.owner.Window.Canvas().Size().Width-280), 42))
}
