package ui

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

func (t *tableWorkspace) tableGrid() fyne.CanvasObject {
	var grid *dataGrid
	model := gridModel{columns: t.page.Result.Columns, info: t.page.Info, length: func() int { return len(t.page.Result.Rows) + len(t.inserts) }, value: t.cellValue, selected: t.selected, selectRow: func(row int, value bool) { t.selected[row] = value }, inspect: func(row, col int) { t.owner.showCell(t.page.Result.Columns[col].Name, t.cellValue(row, col)) }, changed: func(row int) string {
		if row >= len(t.page.Result.Rows) {
			return "insert"
		}
		return t.edits[row].Kind
	}}
	model.sortColumn = t.applyQuickSort
	model.sorts = t.request.Sorts
	model.copyValue = t.owner.Window.Clipboard().SetContent
	model.current = func() bool { return t.grid == grid && !t.closed && !t.busy }
	model.pending = t.updateEditStatus
	model.canEditCell = func(row, col int) bool {
		for _, field := range t.page.Info.Columns {
			if field.Name == t.page.Result.Columns[col].Name {
				return domain.WritableColumn(field)
			}
		}
		return false
	}
	if !t.profile.ReadOnly && !t.profile.Config.Protection.RestrictDataEdit && !t.page.Result.Truncated && t.object.Kind != "view" {
		model.edit = t.editCell
	}
	grid = newDataGrid(model)
	t.grid = grid
	return t.grid
}
func (t *tableWorkspace) cellValue(row, col int) any {
	name := t.page.Result.Columns[col].Name
	if row >= len(t.page.Result.Rows) {
		return t.inserts[row-len(t.page.Result.Rows)].Values[name]
	}
	if change, ok := t.edits[row]; ok {
		if value, exists := change.Values[name]; exists {
			return value
		}
	}
	return t.page.Result.Rows[row][col]
}
func (t *tableWorkspace) originalRow(row int) map[string]any {
	values := make(map[string]any, len(t.page.Result.Columns))
	for col, column := range t.page.Result.Columns {
		values[column.Name] = t.page.Result.Rows[row][col]
	}
	return values
}
func (t *tableWorkspace) editCell(row, col int, text string, isNull bool) error {
	if t.closed || t.uncertain || t.busy || t.profile.ReadOnly || t.profile.Config.Protection.RestrictDataEdit || t.page.Result.Truncated {
		return domain.ErrReadOnly
	}
	if row < 0 || row >= len(t.page.Result.Rows)+len(t.inserts) || col < 0 || col >= len(t.page.Result.Columns) {
		return errors.New("cell no longer exists")
	}
	column := t.page.Result.Columns[col].Name
	var value any = text
	for _, metadata := range t.page.Info.Columns {
		if metadata.Name == column {
			if !domain.WritableColumn(metadata) {
				t.status.SetText("计算字段由数据库生成，不能编辑。")
				return domain.ErrReadOnly
			}
			if isNull {
				if metadata.Nullable != "YES" {
					t.status.SetText("此字段不允许 NULL")
					return errors.New("field is not nullable")
				}
				value = nil
			} else {
				var err error
				value, err = parseCellValue(metadata.Type, text)
				if err != nil {
					t.status.SetText("值格式错误：" + err.Error())
					return err
				}
			}
			break
		}
	}
	if row >= len(t.page.Result.Rows) {
		t.inserts[row-len(t.page.Result.Rows)].Values[column] = value
	} else {
		change := t.edits[row]
		if change.Kind == "delete" {
			return errors.New("row is staged for deletion")
		}
		if change.Values == nil {
			change = domain.RowChange{Kind: "update", Original: t.originalRow(row), Values: map[string]any{}}
		}
		if reflect.DeepEqual(change.Original[column], value) {
			delete(change.Values, column)
		} else {
			change.Values[column] = value
		}
		if len(change.Values) == 0 {
			delete(t.edits, row)
		} else {
			t.edits[row] = change
		}
	}
	t.updateEditStatus()
	if t.grid != nil {
		t.grid.Refresh()
	}
	return nil
}
func parseCellValue(kind, text string) (any, error) {
	return sqlworkbench.ConvertValue(kind, text)
}
func (t *tableWorkspace) addRow() {
	if t.busy || t.profile.ReadOnly || t.profile.Config.Protection.RestrictDataEdit || t.object.Kind == "view" {
		return
	}
	if !t.stageEditors() {
		return
	}
	if len(t.inserts)+len(t.edits) >= 1000 {
		t.status.SetText("每次最多暂存 1,000 行")
		return
	}
	t.inserts = append(t.inserts, domain.RowChange{Kind: "insert", Values: map[string]any{}})
	t.updateEditStatus()
	t.grid.Refresh()
	t.grid.ScrollTo(widget.TableCellID{Row: len(t.page.Result.Rows) + len(t.inserts) - 1, Col: 2})
}
func (t *tableWorkspace) deleteRows() {
	if t.busy || t.profile.ReadOnly || t.profile.Config.Protection.RestrictDataEdit || t.object.Kind == "view" {
		return
	}
	if !t.stageEditors() {
		return
	}
	remaining := make([]domain.RowChange, 0, len(t.inserts))
	for i, row := range t.inserts {
		if !t.selected[len(t.page.Result.Rows)+i] {
			remaining = append(remaining, row)
		}
	}
	t.inserts = remaining
	for row, selected := range t.selected {
		if selected && row < len(t.page.Result.Rows) {
			t.edits[row] = domain.RowChange{Kind: "delete", Original: t.originalRow(row)}
		}
	}
	t.selected = map[int]bool{}
	t.updateEditStatus()
	t.body.Objects = []fyne.CanvasObject{t.tableGrid()}
	t.body.Refresh()
}
func (t *tableWorkspace) changes() domain.TableChanges {
	rows := append([]domain.RowChange(nil), t.inserts...)
	for i := range t.page.Result.Rows {
		if change, ok := t.edits[i]; ok {
			rows = append(rows, change)
		}
	}
	return domain.TableChanges{Object: t.object, Revision: t.page.Revision, Rows: rows}
}
func (t *tableWorkspace) dirty() bool {
	return len(t.inserts)+len(t.edits) > 0 || t.grid.hasPendingEditor()
}
func (t *tableWorkspace) updateEditStatus() {
	t.status.SetText(fmt.Sprintf("待提交：新增 %d 行 · 修改/删除 %d 行", len(t.inserts), len(t.edits)))
	if t.commitButton != nil {
		if t.dirty() && !t.busy && !t.uncertain {
			t.commitButton.Enable()
		} else {
			t.commitButton.Disable()
		}
	}
}
func (t *tableWorkspace) discardEdits() {
	if t.busy {
		return
	}
	dialog.ShowConfirm("丢弃未提交修改", "这会清除当前表的暂存修改。", func(ok bool) {
		if ok {
			t.clearEdits()
			t.showPage()
		}
	}, t.owner.Window)
}
func (t *tableWorkspace) clearEdits() {
	t.grid.discardEditors()
	t.uncertain = false
	clear(t.edits)
	t.inserts = nil
	clear(t.selected)
	t.updateEditStatus()
}
func (t *tableWorkspace) previewChanges() {
	if !t.stageEditors() {
		return
	}
	changes := t.changes()
	if len(changes.Rows) == 0 {
		return
	}
	statements, err := sqlworkbench.BuildChanges(t.profile.SQLDialect(), changes, t.page.Info)
	if err != nil {
		t.status.SetText(err.Error())
		return
	}
	entry := widget.NewMultiLineEntry()
	entry.TextStyle = fyne.TextStyle{Monospace: true}
	entry.SetText(sqlworkbench.PreviewChanges(statements))
	entry.Disable()
	d := dialog.NewCustom("待提交 SQL", "关闭", entry, t.owner.Window)
	d.Resize(fyne.NewSize(860, 560))
	d.Show()
}
func (t *tableWorkspace) submitChanges(confirmation string) {
	if !t.stageEditors() {
		return
	}
	if t.busy || !t.dirty() || t.profile.ReadOnly || t.profile.Config.Protection.RestrictDataEdit || t.uncertain {
		return
	}
	changes := t.changes()
	changes.Confirmation = confirmation
	t.busy = true
	t.commitButton.Disable()
	profileID := t.profile.ID
	t.owner.jobs.runWithCancel(&t.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return t.owner.Engine.ApplyTableChanges(ctx, profileID, changes)
	}, func(value any, err error) {
		if t.closed {
			return
		}
		t.busy = false
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			t.updateEditStatus()
			statements, previewErr := sqlworkbench.BuildChanges(t.profile.SQLDialect(), changes, t.page.Info)
			if previewErr != nil {
				t.status.SetText(previewErr.Error())
				return
			}
			entry := widget.NewMultiLineEntry()
			entry.TextStyle = fyne.TextStyle{Monospace: true}
			entry.SetText(sqlworkbench.PreviewChanges(statements))
			entry.Disable()
			content := container.NewBorder(widget.NewLabel(fmt.Sprintf("连接：%s · %s\n表：%s · %d 项修改", t.profile.Name, t.profile.Environment, t.object.Name, len(changes.Rows))), nil, nil, nil, entry)
			d := dialog.NewCustomConfirm("确认提交数据修改", "提交", "取消", content, func(ok bool) {
				if ok {
					t.submitChanges(required.Fingerprint)
				}
			}, t.owner.Window)
			d.Resize(fyne.NewSize(860, 600))
			d.Show()
			return
		}
		var warning *domain.CommittedWarning
		if errors.As(err, &warning) {
			t.clearEdits()
			t.refresh()
			dialog.ShowInformation("修改已提交", warning.Error(), t.owner.Window)
			return
		}
		if err != nil {
			t.updateEditStatus()
			t.uncertain = errors.Is(err, domain.ErrUnknownOutcome)
			t.updateEditStatus()
			t.status.SetText("提交失败：" + err.Error())
			return
		}
		t.clearEdits()
		t.status.SetText(fmt.Sprintf("已提交 %d 行", value.(int64)))
		t.refresh()
	})
}
