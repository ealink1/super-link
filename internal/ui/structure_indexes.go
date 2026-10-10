package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (d *tableDesigner) indexPanel() fyne.CanvasObject {
	selected := map[int]bool{}
	result := domain.Result{Columns: []domain.Column{{Name: "索引名"}, {Name: "字段"}, {Name: "索引类型"}, {Name: "唯一性"}, {Name: "状态"}}}
	existing := indexDisplayRows(d.info.Indexes)
	fieldCount := len(d.info.Indexes)
	for _, index := range existing {
		state := "正常"
		for _, change := range d.indexChanges {
			if change.Kind == "dropIndex" && change.OriginalName == index.name {
				state = "待删除"
			}
		}
		result.Rows = append(result.Rows, []any{index.name, index.columns, index.method, indexUniqueness(index.unique), state})
	}
	for _, change := range d.indexChanges {
		if change.Kind == "addIndex" {
			names := []string{}
			for _, c := range change.Index.Columns {
				name := c.Name
				if c.Descending {
					name += " DESC"
				}
				names = append(names, name)
			}
			fieldCount += len(change.Index.Columns)
			result.Rows = append(result.Rows, []any{change.Index.Name, strings.Join(names, ", "), change.Index.Method, indexUniqueness(change.Index.Unique), "待新增"})
		}
	}
	model := gridModel{columns: result.Columns, length: func() int { return len(result.Rows) }, value: func(row, col int) any { return result.Rows[row][col] }, selected: selected, selectRow: func(row int, v bool) { selected[row] = v }, inspect: func(row, col int) { d.owner.showCell(result.Columns[col].Name, result.Rows[row][col]) }}
	model.changed = func(row int) string {
		if row >= len(existing) {
			return "insert"
		}
		for _, change := range d.indexChanges {
			if change.Kind == "dropIndex" && change.OriginalName == existing[row].name {
				return "delete"
			}
		}
		return ""
	}
	tools := container.NewHBox(action("新建索引", "add-row", d.addIndex), action("删除选中索引", "trash", func() { d.removeIndexes(selected, false) }), action("撤销选中删除", "undo", func() { d.removeIndexes(selected, true) }), layout.NewSpacer())
	stats := widget.NewLabel(fmt.Sprintf("索引数：%d，索引字段数：%d", len(result.Rows), fieldCount))
	return container.NewBorder(container.NewVBox(tools, stats), nil, nil, nil, newIndexListGrid(model))
}

type indexField struct {
	column     *widget.Select
	descending *widget.Check
}

func (d *tableDesigner) addIndex() {
	if !d.canEdit() || !d.stageEditors() {
		return
	}
	name := widget.NewEntry()
	name.SetPlaceHolder("索引名称")
	unique := widget.NewCheck("唯一索引", nil)
	methods := []string{"BTREE"}
	if d.profile.SQLDialect() == "postgres" {
		methods = append(methods, "HASH", "GIN", "GIST", "BRIN", "SPGIST")
	} else if d.profile.SQLDialect() == "mysql" {
		methods = append(methods, "HASH")
	}
	method := widget.NewSelect(methods, nil)
	method.SetSelected("BTREE")
	columns := []string{}
	for _, f := range d.fields {
		if !f.deleted && f.column.Name != "" {
			columns = append(columns, f.column.Name)
		}
	}
	rows := container.NewVBox()
	fields := []indexField{}
	var updatePreview func()
	add := func() {
		if len(fields) >= 32 {
			return
		}
		picker := widget.NewSelect(columns, nil)
		picker.PlaceHolder = "选择字段"
		if len(columns) > 0 {
			picker.SetSelected(columns[0])
		}
		order := widget.NewCheck("降序", nil)
		fields = append(fields, indexField{column: picker, descending: order})
		rows.Add(container.NewBorder(nil, nil, nil, order, picker))
		picker.OnChanged = func(string) {
			if updatePreview != nil {
				updatePreview()
			}
		}
		order.OnChanged = func(bool) {
			if updatePreview != nil {
				updatePreview()
			}
		}
		if updatePreview != nil {
			updatePreview()
		}
	}
	add()
	form := container.NewVBox(widget.NewForm(widget.NewFormItem("名称", name), widget.NewFormItem("类型", method)), unique, widget.NewLabel("字段顺序与列表顺序一致"), rows, action("添加字段", "add-row", add))
	indexValue := func() domain.NewIndex {
		index := domain.NewIndex{Name: name.Text, Unique: unique.Checked, Method: method.Selected}
		for _, field := range fields {
			index.Columns = append(index.Columns, domain.IndexColumn{Name: field.column.Selected, Descending: field.descending.Checked})
		}
		return index
	}
	preview, submit := widget.NewMultiLineEntry(), widget.NewButton("暂存", nil)
	preview.TextStyle = fyne.TextStyle{Monospace: true}
	preview.SetMinRowsVisible(6)
	preview.Disable()
	updatePreview = func() { d.previewNewIndex(indexValue(), preview, submit) }
	name.OnChanged = func(string) { updatePreview() }
	method.OnChanged = func(string) { updatePreview() }
	unique.OnChanged = func(bool) { updatePreview() }
	form.Add(widget.NewLabel("SQL 预览"))
	form.Add(preview)
	form.Add(submit)
	modal := dialog.NewCustom("新建索引", "取消", container.NewVScroll(form), d.owner.Window)
	submit.OnTapped = func() {
		if d.stageNewIndex(indexValue()) {
			modal.Hide()
		}
	}
	updatePreview()
	modal.Resize(fyne.NewSize(630, 620))
	modal.Show()
}

func indexUniqueness(unique bool) string {
	if unique {
		return "唯一"
	}
	return "普通"
}
