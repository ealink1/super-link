package ui

import (
	"fmt"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func TestCatalogMouseDownSelectsImmediatelyWithoutCellRebuild(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	object := page.shown[0]
	updates := 0
	update := page.list.UpdateCell
	page.list.UpdateCell = func(id widget.TableCellID, cell fyne.CanvasObject) { updates++; update(id, cell) }
	row := &databaseTableRow{object: object, selectRow: page.selectTableRow, beginSelection: page.beginCatalogSelection}
	press := &desktop.MouseEvent{Button: desktop.MouseButtonPrimary, Modifier: fyne.KeyModifierControl}
	row.MouseDown(press)
	if !page.selectedTables[object] {
		t.Fatal("selection waits for delayed tap")
	}
	row.MouseUp(press)
	row.Tapped(&fyne.PointEvent{})
	if !page.selectedTables[object] {
		t.Fatal("delayed tap toggled immediate selection again")
	}
	row.MouseDown(press)
	if page.selectedTables[object] {
		t.Fatal("Ctrl deselection not immediate")
	}
	row.MouseUp(press)
	row.Tapped(&fyne.PointEvent{})
	page.hoverTableRow(object, true)
	page.hoverTableRow(object, false)
	if updates != 0 {
		t.Fatalf("selection/hover rebuilt %d table cells", updates)
	}
}

func TestCatalogDragSelectsRangeAndPreservesControlBaseline(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	page.objects = nil
	for i := 0; i < 6; i++ {
		page.objects = append(page.objects, domain.Object{Kind: "table", Scope: "main", Name: fmt.Sprintf("table_%d", i)})
	}
	page.filter()
	row := &databaseTableRow{object: page.shown[1], rowIndex: 1, selectRow: page.selectTableRow, beginSelection: page.beginCatalogSelection, dragSelection: page.dragCatalogSelection, endSelection: page.endCatalogSelection}
	row.Resize(fyne.NewSize(100, 30))
	press := &desktop.MouseEvent{PointEvent: fyne.PointEvent{AbsolutePosition: fyne.NewPos(100, 100), Position: fyne.NewPos(20, 15)}, Button: desktop.MouseButtonPrimary}
	row.MouseDown(press)
	row.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{AbsolutePosition: fyne.NewPos(100, 160)}})
	if len(page.selectedTables) != 3 || !page.selectedTables[page.shown[1]] || !page.selectedTables[page.shown[3]] {
		t.Fatal("forward drag missed range", page.selectedTables)
	}
	row.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{AbsolutePosition: fyne.NewPos(100, 70)}})
	if len(page.selectedTables) != 2 || !page.selectedTables[page.shown[0]] || page.selectedTables[page.shown[3]] {
		t.Fatal("reverse drag retained old range", page.selectedTables)
	}
	row.DragEnd()
	row.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{AbsolutePosition: fyne.NewPos(100, 250)}})
	if len(page.selectedTables) != 2 {
		t.Fatal("selection changed after drag end")
	}
	page.selectTableRow(page.shown[5], 0)
	press.Modifier = fyne.KeyModifierControl
	row.MouseDown(press)
	row.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{AbsolutePosition: fyne.NewPos(100, 160)}})
	if len(page.selectedTables) != 4 || !page.selectedTables[page.shown[5]] {
		t.Fatal("Ctrl drag lost baseline", page.selectedTables)
	}
	row.MouseUp(press)
	if page.dragSelection != nil {
		t.Fatal("release retained drag session")
	}
}
