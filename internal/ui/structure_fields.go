package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func (d *tableDesigner) addField() {
	if !d.canEdit() {
		return
	}
	if len(d.fields) >= 1024 {
		d.status.SetText("设计器最多显示 1,024 个字段")
		return
	}
	d.fields = append(d.fields, designColumn{column: connection.ColumnDefinition{Type: "TEXT", Nullable: "YES"}})
	d.rebuildFields()
	d.status.SetText("新增字段已暂存，请填写名称、类型与长度。")
}
func (d *tableDesigner) deleteFields() {
	if !d.canEdit() {
		return
	}
	for row, value := range d.selected {
		if value && row < len(d.fields) {
			d.fields[row].deleted = !d.fields[row].deleted
		}
	}
	clear(d.selected)
	d.rebuildFields()
	d.status.SetText("字段删除已暂存；保存后可能丢失该字段的数据。")
}
func (d *tableDesigner) copyFields() {
	d.copied = nil
	for row, f := range d.fields {
		if d.selected[row] && !f.deleted {
			d.copied = append(d.copied, f.column)
		}
	}
	d.status.SetText("已复制选中的字段定义，可粘贴为新字段。")
}
func (d *tableDesigner) pasteFields() {
	if !d.canEdit() || len(d.fields)+len(d.copied) > 1024 {
		return
	}
	for _, c := range d.copied {
		c.Name += "_copy"
		c.Key = ""
		c.Extra = ""
		d.fields = append(d.fields, designColumn{column: c})
	}
	d.rebuildFields()
}
func ddlView(sql string) fyne.CanvasObject {
	text := widget.NewMultiLineEntry()
	text.TextStyle = fyne.TextStyle{Monospace: true}
	text.SetText(sql)
	text.Disable()
	return text
}
func (w *Window) foreignKeysView(info domain.TableInfo) fyne.CanvasObject {
	result := domain.Result{Columns: []domain.Column{{Name: "名称"}, {Name: "字段"}, {Name: "引用表"}, {Name: "引用字段"}}}
	for _, f := range info.ForeignKeys {
		result.Rows = append(result.Rows, []any{f.Name, f.ColumnName, f.RefTableName, f.RefColumnName})
	}
	return w.resultView(result)
}
func (w *Window) triggersView(info domain.TableInfo) fyne.CanvasObject {
	result := domain.Result{Columns: []domain.Column{{Name: "名称"}, {Name: "时机"}, {Name: "事件"}, {Name: "语句"}}}
	for _, t := range info.Triggers {
		result.Rows = append(result.Rows, []any{t.Name, t.Timing, t.Event, t.Statement})
	}
	return w.resultView(result)
}
