package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (p *databaseTables) tableMenu(object domain.Object) *fyne.Menu {
	return fyne.NewMenu("数据表",
		fyne.NewMenuItem("打开表数据", func() { p.owner.openTable(p.profile, object).views.SelectIndex(0) }),
		fyne.NewMenuItem("查看表结构", func() { p.owner.openTable(p.profile, object).views.SelectIndex(1) }),
		fyne.NewMenuItem("查看 DDL", func() { p.owner.openTable(p.profile, object).views.SelectIndex(2) }),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("复制表名", func() { fyne.CurrentApp().Clipboard().SetContent(object.Name) }),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("刷新列表", p.refresh),
	)
}

func (p *databaseTables) showTableMenu(object domain.Object, position fyne.Position) {
	widget.ShowPopUpMenuAtPosition(p.tableMenu(object), p.owner.Window.Canvas(), position)
}
