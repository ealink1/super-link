package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"reflect"
	"testing"
)

func TestDatabaseTableGridDisplaysStatistics(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	if page.list.Theme().Size(theme.SizeNamePadding) != 0 {
		t.Fatal("catalog cells have gaps")
	}
	page.statistics = map[string][]any{"items": {int64(589), int64(81920), "InnoDB", "2026-01-05 20:29:11", nil, "utf8mb4_unicode_ci", "表注释"}}
	for column, want := range []string{"public.items", "589", "80 KB", "InnoDB", "2026-01-05 20:29:11", "", "utf8mb4_unicode_ci", "表注释"} {
		cell := page.list.CreateCell()
		page.list.UpdateCell(widget.TableCellID{Row: 0, Col: column}, cell)
		row := cell.(*fyne.Container).Objects[1].(*fyne.Container).Objects[0].(*databaseTableRow)
		if row.Text != want {
			t.Fatalf("column %d: %q, want %q", column, row.Text, want)
		}
		if column == 0 {
			test.Tap(row)
			if page.hoveredTable != "" {
				t.Fatal("click retained highlight")
			}
			row.MouseIn(&desktop.MouseEvent{})
			if len(w.tables) != 0 {
				t.Fatal("single click opened table")
			}
			for col := range tableCatalogHeaders {
				selectedCell := page.list.CreateCell()
				page.list.UpdateCell(widget.TableCellID{Row: 0, Col: col}, selectedCell)
				background := selectedCell.(*fyne.Container).Objects[0].(*canvas.Rectangle)
				if !reflect.DeepEqual(background.FillColor, theme.SelectionColor()) {
					t.Fatalf("column %d missing row selection", col)
				}
			}
			row.MouseOut()
			if page.hoveredTable != "" {
				t.Fatal("hover highlight remained after leaving")
			}
			test.DoubleTap(row)
		}
	}
	waitUI(t, w)
	if len(w.tables) != 1 {
		t.Fatal("table cell double click ignored")
	}
}
