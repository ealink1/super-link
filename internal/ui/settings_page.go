package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (w *Window) settingsPage() {
	for _, item := range w.tabs.Items {
		if item.Text == "设置中心" {
			w.tabs.Select(item)
			return
		}
	}
	body := container.NewStack()
	themePage := func() fyne.CanvasObject {
		dark := w.appearanceCheck()
		return container.NewVBox(widget.NewLabelWithStyle("主题与外观", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), dark, widget.NewLabel("独立 Fyne 工作区 · 原生桌面界面"))
	}
	updatePage := func() fyne.CanvasObject {
		return container.NewVBox(widget.NewLabelWithStyle("SuperLink", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), widget.NewLabel("当前版本 "+w.Version), widget.NewLabel("更新仓库 ealink1/super-link"), widget.NewButton("关于 SuperLink", w.about), widget.NewButton("检查更新", w.checkUpdates), widget.NewButton("驱动管理", w.driverManager))
	}
	connections := func() fyne.CanvasObject {
		return container.NewVBox(widget.NewLabel("连接与配置"), widget.NewLabel("数据目录："+w.Root), widget.NewButton("新建连接", func() { w.editProfile(domain.Profile{}) }), widget.NewButton("管理连接分组", w.groupManager), widget.NewButton("管理查询草稿", w.draftManager))
	}
	items := []string{"主题与外观", "连接与配置", "驱动管理", "SQL 执行历史", "关于与更新"}
	nav := widget.NewList(func() int { return len(items) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(i int, item fyne.CanvasObject) { item.(*widget.Label).SetText(items[i]) })
	nav.OnSelected = func(i int) {
		var content fyne.CanvasObject
		switch i {
		case 0:
			content = themePage()
		case 1:
			content = connections()
		case 2:
			content = container.NewVBox(widget.NewLabel("驱动管理"), widget.NewButton("打开驱动管理", w.driverManager))
		case 3:
			content = container.NewVBox(widget.NewLabel("执行历史"), widget.NewButton("查看执行历史", w.showExecutionHistory))
		default:
			content = updatePage()
		}
		body.Objects = []fyne.CanvasObject{container.NewPadded(content)}
		body.Refresh()
	}
	split := container.NewHSplit(nav, body)
	split.Offset = 0.23
	item := container.NewTabItem("设置中心", split)
	w.tabs.Append(item)
	w.tabs.Select(item)
	nav.Select(4)
	w.syncDocuments()
}

func (w *Window) documentMenu() {
	menu := fyne.NewMenu("工作区", fyne.NewMenuItem("新建查询", w.newSelectedQuery), fyne.NewMenuItem("查询草稿", w.draftManager), fyne.NewMenuItem("关闭当前标签", func() {
		if selected := w.tabs.Selected(); selected != nil {
			w.closeTab(selected)
		}
	}))
	widget.ShowPopUpMenuAtPosition(menu, w.Window.Canvas(), fyne.NewPos(w.Window.Canvas().Size().Width-190, 80))
}
func (w *Window) showExecutionHistory() {
	if s := w.workspaces[w.tabs.Selected()]; s != nil {
		s.history()
		return
	}
	if _, ok := w.selectedProfile(); ok {
		w.openSelected()
		if s := w.workspaces[w.tabs.Selected()]; s != nil {
			s.history()
		}
	}
}
func (w *Window) workflowMenu() {
	menu := fyne.NewMenu("数据工作流", fyne.NewMenuItem("数据导入", func() {
		if t := w.tables[w.tabs.Selected()]; t != nil {
			w.importTable(t.profile, t.object, t.page.Info)
			return
		}
		showInformationDialog("数据导入", "先在对象树打开目标表，再启动导入。", w.Window)
	}), fyne.NewMenuItem("导出当前结果", func() {
		if t := w.tables[w.tabs.Selected()]; t != nil {
			t.export()
			return
		}
		if s := w.workspaces[w.tabs.Selected()]; s != nil {
			s.exportResults()
		}
	}), fyne.NewMenuItem("打开查询草稿", w.draftManager))
	widget.ShowPopUpMenuAtPosition(menu, w.Window.Canvas(), fyne.NewPos(400, 42))
}
