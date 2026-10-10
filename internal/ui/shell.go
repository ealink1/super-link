package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func action(label, name string, run func()) *widget.Button {
	var resource fyne.Resource
	if name != "" {
		resource = icon(name)
	}
	button := widget.NewButtonWithIcon(label, resource, run)
	button.Importance = widget.LowImportance
	return button
}

func (w *Window) buildSQLWorkspace() fyne.CanvasObject {
	w.sidebar = newNavigator(w)
	w.buildDocuments()
	w.tabs.OnSelected = func(*container.TabItem) { w.syncDocuments() }
	w.tabs.OnUnselected = func(*container.TabItem) {}
	query := headerAction("新建查询", w.newSelectedQuery)
	connection := headerAction("新建连接", func() { w.editProfile(domain.Profile{}) })
	header := container.NewHBox(query, connection,
		headerAction("管理连接分组", w.groupManager), headerDivider(), headerAction("执行历史", w.showExecutionHistory), layout.NewSpacer())
	header.Layout = &toolbarLayout{height: 56}
	sidebar := container.New(layout.NewCustomPaddedLayout(0, 0, 0, 6), w.sidebar.content())
	documents := container.New(layout.NewCustomPaddedLayout(0, 0, 6, 0), w.docHost)
	main := container.NewHSplit(sidebar, documents)
	main.Offset = 0.18
	w.syncDocuments()
	return container.NewStack(container.NewBorder(container.NewVBox(header, widget.NewSeparator()), nil, nil, nil, main), w.docTooltip.layer)
}
