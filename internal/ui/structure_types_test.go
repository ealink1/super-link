package ui

import (
	"testing"

	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func TestFieldTypeDropdownStagesSelectedType(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	value := "VARCHAR(64)"
	model := gridModel{columns: []domain.Column{{Name: "类型"}}, selected: map[int]bool{}, value: func(int, int) any { return value }, choices: map[int][]string{0: tableColumnTypes("mysql")}, edit: func(row, col int, text string, _ bool) error { value = text; return nil }}
	cell := newGridCell(model)
	cell.bind(widget.TableCellID{Row: 0, Col: 2})
	if !cell.choice.Visible() || cell.choice.Selected != "VARCHAR(64)" {
		t.Fatal("custom type not retained in dropdown")
	}
	cell.choice.SetSelected("BIGINT")
	if value != "BIGINT" || cell.choice.Selected != "BIGINT" {
		t.Fatal("dropdown did not stage type")
	}
	cell.bind(widget.TableCellID{Row: 1, Col: 2})
	if cell.choice.Selected != "BIGINT" {
		t.Fatal("recycled dropdown lost value")
	}
	model.edit = nil
	readonly := newGridCell(model)
	readonly.bind(widget.TableCellID{Row: 0, Col: 2})
	if readonly.choice.Visible() {
		t.Fatal("read-only type editable")
	}
}
