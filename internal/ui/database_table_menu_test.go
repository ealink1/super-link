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
	menu.Items[4].Action()
	if fyne.CurrentApp().Clipboard().Content() != object.Name {
		t.Fatal("table name was not copied")
	}
	for index := 0; index < 3; index++ {
		menu.Items[index].Action()
		waitUI(t, w)
		if len(w.tables) != 1 {
			t.Fatal("menu opened duplicate table tabs")
		}
		for _, table := range w.tables {
			if table.views.SelectedIndex() != index {
				t.Fatalf("menu action %d selected wrong view", index)
			}
		}
	}
}
