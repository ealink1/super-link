package ui

import (
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/widget"
)

var tableActionHints = map[string]string{
	"sort-asc":     "正序排序\n选择字段，按升序读取整张表并返回第一页。保留当前筛选条件。",
	"sort-desc":    "倒序排序\n选择字段，按降序读取整张表并返回第一页。保留当前筛选条件。",
	"refresh":      "刷新数据\n重新读取当前页及表信息；有未提交修改时先确认处理方式。",
	"filter":       "筛选与排序\n展开或收起条件面板，可按字段、手动只读条件和排序限制读取范围。",
	"add-row":      "新增行\n添加一行待提交数据，填写后通过保存按钮提交。",
	"trash":        "删除选中行\n将选中行标记为待删除；点击保存并确认后才写入数据库。",
	"cell-select":  "预览修改\n查看暂存的新增、修改与删除及即将执行的 SQL。",
	"save":         "保存修改\n提交暂存的数据变更；需要可写连接、安全行定位并通过写入确认。",
	"rollback":     "放弃修改\n撤销当前尚未提交的编辑、新增和删除，恢复已读取的数据。",
	"table-design": "表结构设计\n查看并编辑支持的字段和索引，预览 DDL 后确认保存。",
	"copy":         "复制表名\n将当前表的名称复制到剪贴板。",
	"export":       "导出数据\n选择格式与列，将当前数据或支持的完整查询结果导出为文件。",
	"sql-doc":      "新建查询\n为当前表创建查询标签，保留连接、数据库及表上下文。",
	"import":       "导入数据\n打开导入向导，预览文件、映射目标列并确认分批写入。",
}

type tableActionButton struct {
	widget.Button
	hint    string
	tooltip *documentTooltip
}

func (w *Window) tableAction(name string, run func()) *tableActionButton {
	button := &tableActionButton{hint: tableActionHints[name], tooltip: w.docTooltip}
	button.Text, button.Icon, button.OnTapped, button.Importance = "", icon(name), run, widget.LowImportance
	switch name {
	case "sort-asc":
		button.Icon = headerOutlineIcon(name, `<path d="M6 20V4m-4 4 4-4 4 4M14 6h6M14 12h4M14 18h2"/>`)
	case "sort-desc":
		button.Icon = headerOutlineIcon(name, `<path d="M6 4v16m-4-4 4 4 4-4M14 6h2M14 12h4M14 18h6"/>`)
	}
	button.ExtendBaseWidget(button)
	return button
}
func (b *tableActionButton) MouseIn(event *desktop.MouseEvent) {
	b.Button.MouseIn(event)
	if b.tooltip != nil {
		b.tooltip.showContentAt(b, b.hint, event)
	}
}
func (b *tableActionButton) MouseMoved(event *desktop.MouseEvent) {
	if b.tooltip != nil && b.tooltip.active == b {
		b.tooltip.moveBelowPointer(event)
	}
}
func (b *tableActionButton) MouseOut() {
	b.Button.MouseOut()
	if b.tooltip != nil && b.tooltip.active == b {
		b.tooltip.hide()
	}
}
