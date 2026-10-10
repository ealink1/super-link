package ui

import "fyne.io/fyne/v2/widget"

var navigatorActionHints = map[string]string{
	"search":          "搜索连接与对象\n展开或收起搜索框，按名称筛选侧栏中的连接和对象。",
	"locate":          "定位选中项\n滚动侧栏，让当前选中的连接或对象显示在可见区域。",
	"refresh":         "刷新连接与对象\n重新读取当前选中节点，更新侧栏列表。",
	"connection-menu": "连接操作\n打开新建查询、编辑、断开或删除连接等操作。",
	"all-objects":     "全部对象\n取消对象类型筛选，显示所有类型的数据库对象。",
	"table":           "数据表\n仅显示数据表，隐藏其他类型的数据库对象。",
	"view":            "视图\n仅显示数据库视图。",
	"function":        "函数\n仅显示数据库函数。",
	"sql-doc":         "已存查询\n打开已保存的 SQL 查询列表。",
	"history":         "执行历史\n查看当前连接最近的 SQL 执行记录。",
}

func (n *navigator) hintedAction(name string, run func()) *tableActionButton {
	button := &tableActionButton{hint: navigatorActionHints[name], tooltip: n.owner.docTooltip}
	button.Icon, button.Importance = icon(name), widget.LowImportance
	button.OnTapped = func() {
		if button.tooltip != nil {
			button.tooltip.hide()
		}
		if run != nil {
			run()
		}
	}
	button.ExtendBaseWidget(button)
	return button
}
