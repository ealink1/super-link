package ui

import (
	"fyne.io/fyne/v2"
	"testing"
)

func TestDatabaseTableMenuActions(t *testing.T) {
	w, profile := parityWindow(t)
	page := w.openDatabaseTables(profile, "main")
	waitUI(t, w)
	object := page.shown[0]
	menu := page.tableMenu(object)
	var export *fyne.MenuItem
	for _, item := range menu.Items {
		if item.Label == "导出数据" {
			export = item
		}
	}
	if export == nil || export.Action == nil || export.Disabled {
		t.Fatal("table menu is missing an enabled export action")
	}
	export.Action()
	waitUI(t, w)
	if len(w.Window.Canvas().Overlays().List()) == 0 {
		t.Fatal("export action did not open its dialog")
	}
	for _, item := range menu.Items {
		if item.Label == "复制名称" {
			item.Action()
		}
	}
	if fyne.CurrentApp().Clipboard().Content() != object.Name {
		t.Fatalf("table name was not copied: got %q want %q items %v", fyne.CurrentApp().Clipboard().Content(), object.Name, menu.Items)
	}
	node := &navNode{kind: "object", profileID: profile.ID, scope: object.Scope, object: object}
	treeMenu := w.sidebar.nodeMenu(node)
	if len(menu.Items) != len(treeMenu.Items) {
		t.Fatal("tree and table menus have different actions")
	}
	for index, item := range menu.Items {
		other := treeMenu.Items[index]
		if item.Label != other.Label || item.Disabled != other.Disabled || item.IsSeparator != other.IsSeparator {
			t.Fatalf("menu action %d differs", index)
		}
	}
	for _, item := range menu.Items {
		if item.Label == "查看数据" {
			item.Action()
			waitUI(t, w)
			if len(w.tables) != 1 {
				t.Fatal("data action did not open table")
			}
		}
	}
}
