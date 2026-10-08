package ui

import (
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestDatabaseTablesListsFiltersOpensAndCloses(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	if len(page.objects) != 1 || page.objects[0].Name != "items" || page.objects[0].Scope != "main" {
		t.Fatalf("tables: %+v", page.objects)
	}
	if w.openDatabaseTables(p, "main") != page {
		t.Fatal("duplicate database tab")
	}
	page.search.SetText("absent")
	if len(page.shown) != 0 {
		t.Fatal("filter ignored")
	}
	page.search.SetText("PUBLIC.IT")
	if len(page.shown) != 1 {
		t.Fatal("schema search ignored")
	}
	row := &databaseTableRow{object: page.shown[0], open: func(object domain.Object) { w.openTable(p, object) }}
	row.ExtendBaseWidget(row)
	test.DoubleTap(row)
	waitUI(t, w)
	if len(w.tables) != 1 {
		t.Fatal("double click did not open table")
	}
	w.closeTab(page.item)
	if !page.closed || len(w.databases) != 0 {
		t.Fatal("database page not released")
	}
}

func TestSelectingDatabaseOpensTablesTab(t *testing.T) {
	w, p := parityWindow(t)
	node := &navNode{id: "test-database", kind: "database", profileID: p.ID, scope: "main"}
	w.sidebar.nodes[node.id] = node
	w.sidebar.selectNode(node.id)
	waitUI(t, w)
	if len(w.databases) != 1 || w.tabs.Selected().Text != "main" {
		t.Fatal("database selection did not open a tab")
	}
	w.removeProfileDocuments(p.ID)
	if len(w.databases) != 0 {
		t.Fatal("profile cleanup retained database tab")
	}
}

func TestSelectingTableCategoryReusesDatabaseTab(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	table := w.openTable(p, page.objects[0])
	waitUI(t, w)
	node := &navNode{id: "database/schema/table", kind: "category", profileID: p.ID, scope: "main"}
	w.sidebar.nodes[node.id] = node
	w.sidebar.selectNode(node.id)
	if len(w.databases) != 1 || w.tabs.Selected() != page.item {
		t.Fatal("table category did not select existing catalog")
	}
	w.closeTab(page.item)
	w.tabs.Select(table.item)
	w.sidebar.selectNode(node.id)
	waitUI(t, w)
	if len(w.databases) != 1 || w.tabs.Selected().Text != "main" {
		t.Fatal("table category did not open catalog")
	}
}
