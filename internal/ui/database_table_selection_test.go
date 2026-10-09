package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"reflect"
	"strings"
	"testing"
)

func TestCatalogControlClickTogglesRows(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	first := page.shown[0]
	second := domain.Object{Kind: "table", Scope: "main", Schema: "public", Name: "other"}
	page.objects = append(page.objects, second)
	page.filter()
	click := func(object domain.Object, mod fyne.KeyModifier) {
		row := &databaseTableRow{object: object, selectRow: page.selectTableRow}
		row.MouseDown(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary, Modifier: mod})
		row.MouseUp(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary, Modifier: mod})
		row.Tapped(&fyne.PointEvent{})
	}
	click(first, 0)
	click(second, fyne.KeyModifierControl)
	if len(page.selectedTables) != 2 || !strings.Contains(page.status.Text, "已选 2 张表") {
		t.Fatal("Ctrl did not add row", page.selectedTables)
	}
	for col := range tableCatalogHeaders {
		cell := page.list.CreateCell()
		page.list.UpdateCell(widget.TableCellID{Row: 0, Col: col}, cell)
		background := cell.(*fyne.Container).Objects[0].(*canvas.Rectangle)
		if !reflect.DeepEqual(background.FillColor, theme.SelectionColor()) {
			t.Fatal("row selection not painted across columns")
		}
	}
	click(first, fyne.KeyModifierControl)
	if len(page.selectedTables) != 1 || page.selectedTables[first] {
		t.Fatal("Ctrl did not remove row")
	}
	click(first, fyne.KeyModifierSuper)
	if len(page.selectedTables) != 2 {
		t.Fatal("Command did not add row")
	}
	page.showTableMenu(first, fyne.NewPos(0, 0))
	if len(page.selectedTables) != 2 {
		t.Fatal("right click lost selection")
	}
	click(second, 0)
	if len(page.selectedTables) != 1 || !page.selectedTables[second] {
		t.Fatal("plain click did not select only one row")
	}
	page.search.SetText(first.Name)
	if len(page.selectedTables) != 0 {
		t.Fatal("hidden row retained selection")
	}
}
