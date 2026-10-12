package ui

import (
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func TestGridControlsAllocateOnlyWhenUsedAndSurviveRecycling(t *testing.T) {
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	model := gridModel{columns: []domain.Column{{Name: "text"}, {Name: "enabled"}}, length: func() int { return 1 }, value: func(_ int, col int) any {
		if col == 1 {
			return true
		}
		return "value"
	}, selected: map[int]bool{}, boolColumns: map[int]bool{1: true}, editorDrafts: map[widget.TableCellID]gridEditorDraft{}, edit: func(int, int, string, bool) error { return nil }}
	cell := newGridCell(model)
	cell.bind(widget.TableCellID{Row: 0, Col: 2})
	cell.Resize(fyne.NewSize(180, 28))
	renderer := test.WidgetRenderer(cell)
	renderer.Refresh()
	if cell.entry != nil || cell.check != nil || cell.choice != nil || len(renderer.Objects()) != 3 {
		t.Fatal("plain text cell preallocated unused controls")
	}
	cell.bind(widget.TableCellID{Row: 0, Col: 3})
	if cell.check == nil || !cell.check.Visible() || !cell.check.Checked || cell.entry != nil || cell.choice != nil {
		t.Fatal("boolean cell did not lazily create its checkbox")
	}
	check := cell.check
	cell.bind(widget.TableCellID{Row: 0, Col: 0})
	if cell.check != check || !check.Visible() || check.Checked {
		t.Fatal("recycled row-selection checkbox retained boolean value")
	}
	cell.bind(widget.TableCellID{Row: 0, Col: 2})
	cell.ensureEntry()
	entry := cell.entry
	cell.editing = true
	entry.SetText("edited")
	if !(&dataGrid{model: model, cells: []*gridCell{cell}}).hasPendingEditor() {
		t.Fatal("lazy editor did not stage draft")
	}
	cell.bind(widget.TableCellID{Row: 0, Col: 1})
	cell.ensureEntry()
	if cell.entry != entry || entry.Visible() || check.Visible() {
		t.Fatal("recycled editor was replaced or left visible")
	}
	grid := &dataGrid{model: model, cells: []*gridCell{cell, newGridCell(model)}}
	grid.discardEditors() // Cells that were never edited have no entry.
	if grid.hasPendingEditor() {
		t.Fatal("discard retained an editor draft")
	}
}
