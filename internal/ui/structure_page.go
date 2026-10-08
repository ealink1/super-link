package ui

import (
	"context"
	"fmt"
	"reflect"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

type designColumn struct {
	original string
	column   connection.ColumnDefinition
	deleted  bool
}
type tableDesigner struct {
	owner                   *Window
	profile                 domain.Profile
	object                  domain.Object
	info                    domain.TableInfo
	fields                  []designColumn
	indexChanges            []domain.StructureChange
	selected                map[int]bool
	copied                  []connection.ColumnDefinition
	body                    *fyne.Container
	views                   *resultTabs
	grid                    *dataGrid
	status                  *widget.Label
	saveButton              *widget.Button
	item                    *container.TabItem
	cancel                  context.CancelFunc
	closed, busy, uncertain bool
}

func (w *Window) designTable(p domain.Profile, object domain.Object, info domain.TableInfo) {
	for item, d := range w.designers {
		if d.profile.ID == p.ID && d.object == object && !d.closed {
			w.tabs.Select(item)
			return
		}
	}
	d := &tableDesigner{owner: w, profile: p, object: object, info: info, selected: map[int]bool{}, body: container.NewStack(), status: widget.NewLabel("")}
	d.reset(info)
	d.views = newResultTabs(container.NewTabItem("字段", d.body), container.NewTabItem("索引", d.indexPanel()), container.NewTabItem("外键", w.foreignKeysView(info)), container.NewTabItem("触发器", w.triggersView(info)), container.NewTabItem("DDL", ddlView(info.DDL)))
	d.saveButton = action("保存", "save", func() { d.save("") })
	tools := container.NewHBox(widget.NewLabelWithStyle("设计表 · "+object.Name, fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), layout.NewSpacer(), action("", "refresh", d.refresh), action("新增字段", "add-row", d.addField), action("删除字段", "trash", d.deleteFields), action("复制", "copy", d.copyFields), action("粘贴", "clipboard", d.pasteFields), action("SQL 预览", "sql-doc", d.preview), d.saveButton)
	if p.ReadOnly || p.Config.Protection.RestrictStructureEdit {
		d.saveButton.Disable()
		d.status.SetText("此连接限制结构修改；仍可查看和复制结构。")
	}
	content := container.NewBorder(tools, d.status, nil, nil, d.views)
	d.item = container.NewTabItem("设计表 ("+object.Name+")", content)
	w.designers[d.item] = d
	w.tabs.Append(d.item)
	w.tabs.Select(d.item)
	w.syncDocuments()
}

func (d *tableDesigner) canEdit() bool {
	return !d.closed && !d.busy && !d.uncertain && !d.profile.ReadOnly && !d.profile.Config.Protection.RestrictStructureEdit
}
func (d *tableDesigner) reset(info domain.TableInfo) {
	d.info = info
	d.fields = nil
	d.indexChanges = nil
	d.uncertain = false
	clear(d.selected)
	for _, c := range info.Columns {
		d.fields = append(d.fields, designColumn{original: c.Name, column: c})
	}
	d.rebuildFields()
}
func (d *tableDesigner) rebuildFields() {
	var grid *dataGrid
	columns := []domain.Column{{Name: "名称"}, {Name: "类型"}, {Name: "主键"}, {Name: "自增"}, {Name: "非空"}, {Name: "默认值"}, {Name: "注释"}}
	model := gridModel{columns: columns, length: func() int { return len(d.fields) }, value: d.fieldValue, selected: d.selected, selectRow: func(row int, v bool) { d.selected[row] = v }, copyValue: d.owner.Window.Clipboard().SetContent, boolColumns: map[int]bool{2: true, 3: true, 4: true}, changed: func(row int) string {
		f := d.fields[row]
		if f.deleted {
			return "delete"
		}
		if f.original == "" {
			return "insert"
		}
		return ""
	}}
	model.inspect = func(row, col int) { d.owner.showCell(columns[col].Name, d.fieldValue(row, col)) }
	model.current = func() bool { return d.grid == grid && !d.closed && !d.busy }
	if d.canEdit() {
		model.edit = d.editField
		model.choices = map[int][]string{1: tableColumnTypes(d.profile.SQLDialect())}
	}
	grid = newDataGrid(model)
	d.grid = grid
	d.body.Objects = []fyne.CanvasObject{grid}
	d.body.Refresh()
}
func (d *tableDesigner) fieldValue(row, col int) any {
	c := d.fields[row].column
	switch col {
	case 0:
		return c.Name
	case 1:
		return c.Type
	case 2:
		return c.Key == "PRI"
	case 3:
		return strings.Contains(c.Extra, "auto_increment")
	case 4:
		return c.Nullable == "NO"
	case 5:
		if c.Default != nil {
			return *c.Default
		}
		return nil
	default:
		return c.Comment
	}
}
func (d *tableDesigner) editField(row, col int, text string, isNull bool) error {
	if !d.canEdit() {
		return domain.ErrReadOnly
	}
	f := &d.fields[row]
	if f.deleted {
		return fmt.Errorf("column is staged for deletion")
	}
	flag, err := strconv.ParseBool(text)
	if col >= 2 && col <= 4 && err != nil {
		return err
	}
	switch col {
	case 0:
		f.column.Name = text
	case 1:
		f.column.Type = text
	case 2:
		f.column.Key = ""
		if flag {
			f.column.Key = "PRI"
		}
	case 3:
		f.column.Extra = ""
		if flag {
			f.column.Extra = "auto_increment"
		}
	case 4:
		f.column.Nullable = "YES"
		if flag {
			f.column.Nullable = "NO"
		}
	case 5:
		f.column.Default = nil
		f.column.HasDefault = false
		if !isNull {
			f.column.Default = &text
			f.column.HasDefault = true
		}
	case 6:
		f.column.Comment = text
	}
	d.status.SetText("结构修改已暂存，保存前请查看 SQL 预览。")
	return nil
}
func (d *tableDesigner) dirty() bool { return len(d.changes()) > 0 || d.grid.hasPendingEditor() }
func (d *tableDesigner) changes() []domain.StructureChange {
	drops, edits := []domain.StructureChange{}, []domain.StructureChange{}
	old := map[string]connection.ColumnDefinition{}
	for _, c := range d.info.Columns {
		old[c.Name] = c
	}
	for _, f := range d.fields {
		change := domain.StructureChange{OriginalName: f.original, Column: f.column}
		if f.deleted {
			if f.original != "" {
				change.Kind = "dropColumn"
				drops = append(drops, change)
			}
			continue
		}
		if f.original == "" {
			change.Kind = "addColumn"
			edits = append(edits, change)
		} else if !reflect.DeepEqual(f.column, old[f.original]) {
			change.Kind = "alterColumn"
			edits = append(edits, change)
		}
	}
	for _, change := range d.indexChanges {
		if change.Kind == "dropIndex" {
			drops = append([]domain.StructureChange{change}, drops...)
		} else {
			edits = append(edits, change)
		}
	}
	return append(drops, edits...)
}
func (d *tableDesigner) request() domain.StructureRequest {
	return domain.StructureRequest{Object: d.object, Revision: d.profile.Revision, BeforeHash: domain.StructureHash(d.info), Changes: d.changes()}
}
func (d *tableDesigner) preview() {
	if !d.stageEditors() {
		return
	}
	statements, err := sqlworkbench.BuildStructure(d.profile.SQLDialect(), d.request(), d.info)
	if err != nil {
		d.status.SetText(err.Error())
		return
	}
	modal := dialog.NewCustom("结构变更 SQL", "关闭", ddlView(sqlworkbench.PreviewStructure(statements)), d.owner.Window)
	modal.Resize(fyne.NewSize(920, 600))
	modal.Show()
}
