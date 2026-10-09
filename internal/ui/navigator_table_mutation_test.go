package ui

import (
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestTableMutationMenuProtection(t *testing.T) {
	w, p := parityWindow(t)
	p.Config.Type = "sqlite"
	node := &navNode{kind: "object", profileID: p.ID, object: domain.Object{Kind: "table", Scope: "main", Name: "items"}}
	items := w.sidebar.tableMutationItems(node, p)
	if items[1].Label != "删除表" || items[2].Label != "清空表" || items[3].Label != "截断表" {
		t.Fatal("missing actions")
	}
	if items[1].Disabled || items[2].Disabled || !items[3].Disabled {
		t.Fatal("SQLite capabilities incorrect")
	}
	p.ReadOnly = true
	for _, item := range w.sidebar.tableMutationItems(node, p)[1:] {
		if !item.Disabled {
			t.Fatal("read-only mutation enabled")
		}
	}
	p.ReadOnly = false
	p.Config.Protection.RestrictStructureEdit = true
	items = w.sidebar.tableMutationItems(node, p)
	if !items[1].Disabled || items[2].Disabled {
		t.Fatal("structure protection incorrect")
	}
	p.Config.Protection.RestrictScriptExecution = true
	if !w.sidebar.tableMutationItems(node, p)[2].Disabled {
		t.Fatal("script protection ignored")
	}
}

func TestTableMutationRefreshesCatalogOnlyOnce(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	before := page.generation
	object := page.shown[0]
	w.sidebar.refreshTableMutationViews(p, object, "clear")
	if page.generation != before+1 {
		t.Fatalf("catalog refreshed %d times", page.generation-before)
	}
	waitUI(t, w)
	if page.closed || len(page.shown) == 0 {
		t.Fatal("catalog did not remain available")
	}
}
