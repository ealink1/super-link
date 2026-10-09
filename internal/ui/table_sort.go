package ui

import "github.com/ealink1/super-link/internal/domain"

func (t *tableWorkspace) applyQuickSort(column string, descending bool) {
	if t.closed || !t.stageEditors() || !t.canChangePage() {
		return
	}
	valid := false
	for _, name := range t.columnNames() {
		if name == column {
			valid = true
			break
		}
	}
	if !valid {
		t.status.SetText("请选择有效的排序字段。")
		return
	}
	t.request.Page = 1
	t.request.Sorts = []domain.Sort{{Column: column, Descending: descending}}
	// Keep the existing multi-field sorting panel aligned with the quick action.
	t.sorts = nil
	t.sortRows.Objects = nil
	t.sortRows.Refresh()
	t.addSort()
	t.sorts[0].column.SetSelected(column)
	direction := "ASC"
	if descending {
		direction = "DESC"
	}
	t.sorts[0].direction.SetSelected(direction)
	t.refresh()
}
