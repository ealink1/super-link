package ui

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

type shellSettings struct {
	DefaultShell string
	FontSize     int
}

func (s *shellWorkspace) loadSettings() {
	s.settings = &shellSettings{FontSize: 16}
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.owner.Store.Setting(ctx, "shell.appearance") }, func(value any, err error) {
		if err != nil {
			s.status.SetText(err.Error())
			return
		}
		var stored shellSettings
		if json.Unmarshal([]byte(value.(string)), &stored) == nil && stored.FontSize >= 12 && stored.FontSize <= 20 {
			if stored.DefaultShell != "" && stored.DefaultShell != "/bin/zsh" && stored.DefaultShell != "/bin/bash" {
				return
			}
			*s.settings = stored
		}
	})
}

func (s *shellWorkspace) showSettings() {
	s.page = 2
	defaultShell := widget.NewSelect([]string{"系统默认", "/bin/zsh", "/bin/bash"}, nil)
	defaultShell.SetSelected("系统默认")
	if s.settings.DefaultShell != "" {
		defaultShell.SetSelected(s.settings.DefaultShell)
	}
	fontSize := widget.NewSelect([]string{"12", "13", "14", "15", "16", "18", "20"}, nil)
	fontSize.SetSelected(strconv.Itoa(s.settings.FontSize))
	hint := widget.NewLabel("")
	var save *shellAlignedButton
	save = shellButton("保存设置", "check", true, func() {
		font, _ := strconv.Atoi(fontSize.Selected)
		next := shellSettings{DefaultShell: defaultShell.Selected, FontSize: font}
		if next.DefaultShell == "系统默认" {
			next.DefaultShell = ""
		}
		raw, err := json.Marshal(next)
		if err != nil {
			hint.SetText(err.Error())
			return
		}
		save.Disable()
		s.owner.jobs.run(func(ctx context.Context) (any, error) {
			return nil, s.owner.Store.SetSetting(ctx, "shell.appearance", string(raw))
		}, func(_ any, err error) {
			save.Enable()
			if err != nil {
				hint.SetText(err.Error())
				return
			}
			*s.settings = next
			for _, pane := range s.panes {
				pane.terminal.setTextSize(float32(font))
			}
			hint.SetText("设置已保存")
		})
	})
	sidebar := shellPanel(shellVBox(shellText("设置", 20, true, shellTextColor), shellFixed(layout.NewSpacer(), 0, 16), shellFixed(shellButton("终端与外观", "monitor", true, nil), 0, 40), shellFixed(layout.NewSpacer(), 0, 8), shellText("本机设置", 12, false, shellMutedColor)), shellPanelColor, 0, 16)
	field := func(title, description string, input fyne.CanvasObject) fyne.CanvasObject {
		return shellPanel(shellBorder(nil, nil, shellVBox(shellText(title, 14, true, shellTextColor), shellText(description, 12, false, shellMutedColor)), shellFixed(input, 180, 32), layout.NewSpacer()), shellPanelColor, 8, 16)
	}
	version := shellPanel(shellBorder(nil, nil, shellVBox(shellText("SuperLink", 18, true, shellTextColor), shellText("当前版本  "+s.owner.Version, 12, false, shellMutedColor)), shellOutlined(shellButton("检查更新", "refresh-cw", false, s.owner.checkUpdates)), layout.NewSpacer()), shellPanelColor, 12, 20)
	content := shellVBox(shellText("终端设置", 20, true, shellTextColor), shellFixed(layout.NewSpacer(), 0, 20), shellLine(), shellFixed(layout.NewSpacer(), 0, 24), version, shellFixed(layout.NewSpacer(), 0, 24), field("默认本地壳", "新建本地终端时使用", defaultShell), shellFixed(layout.NewSpacer(), 0, 12), field("终端字体大小", "应用到当前会话和新会话", fontSize), shellFixed(layout.NewSpacer(), 0, 12), field("界面主题", "SQL / Shell 共用日间或夜间模式；标题栏可直接切换", shellButton("切换模式", "", false, s.owner.toggleAppearance)), shellFixed(layout.NewSpacer(), 0, 24), shellText("快捷键", 14, true, shellTextColor), shellText("⌘C / ⌘V 复制与粘贴；Ctrl+C 中断；方向键浏览命令历史", 12, false, shellMutedColor), shellFixed(layout.NewSpacer(), 0, 18), shellHBox(save, shellFixed(layout.NewSpacer(), 12, 0), hint), shellFixed(layout.NewSpacer(), 0, 20), shellText(fmt.Sprintf("当前会话 %d · 回滚历史上限 1000 行", len(s.panes)), 12, false, shellMutedColor))
	s.body.Objects = []fyne.CanvasObject{shellBorder(nil, nil, shellFixed(sidebar, 224, 0), nil, container.NewVScroll(shellInset(content, 32)))}
	s.refreshPage()
}
