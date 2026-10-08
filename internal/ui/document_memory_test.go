package ui

import (
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestRemovedDocumentsClearBackingArrayAndPreserveSelection(t *testing.T) {
	for _, index := range []int{0, 1, 2} {
		t.Run(string(rune('0'+index)), func(t *testing.T) {
			items := []*container.TabItem{container.NewTabItem("a", nil), container.NewTabItem("b", nil), container.NewTabItem("c", nil)}
			backing := append([]*container.TabItem(nil), items...)
			d := &documents{Items: backing, selected: items[index]}
			var unselected *container.TabItem
			d.OnUnselected = func(item *container.TabItem) { unselected = item }
			d.Remove(items[index])
			if len(d.Items) != 2 || backing[2] != nil || unselected != items[index] || d.Selected() != d.Items[min(index, 1)] {
				t.Fatal("removed page retained, or neighboring selection changed")
			}
		})
	}
}

func TestDocumentStripReusesHeadersAndDetachesRemovedCallbacks(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	first := container.NewTabItem("first", widget.NewLabel("one"))
	second := container.NewTabItem("second", widget.NewLabel("two"))
	w := &Window{tabs: &documents{Items: []*container.TabItem{first, second}, selected: first}}
	w.buildDocuments()
	w.tabs.OnSelected = func(*container.TabItem) { w.syncDocuments() }
	w.syncDocuments()
	head, strip := w.docHeader.Objects[0], w.docStrip
	a, b := w.docButtons[first], w.docButtons[second]
	for i := range 100 {
		w.tabs.Select(w.tabs.Items[i%2])
		w.syncDocuments()
	}
	if w.docButtons[first] != a || w.docButtons[second] != b || w.docStrip != strip || w.docHeader.Objects[0] != head {
		t.Fatal("page refresh recreated header widgets")
	}
	first.Text = "renamed"
	w.tabs.Items = []*container.TabItem{second, first}
	w.syncDocuments()
	test.Tap(a)
	if w.tabs.Selected() != first || a.title != "renamed" || !test.WidgetRenderer(a).(*documentTabRenderer).indicator.Visible() || test.WidgetRenderer(b).(*documentTabRenderer).indicator.Visible() {
		t.Fatal("renaming, reordering or selection highlight regressed")
	}
	w.tabs.Remove(first)
	w.syncDocuments()
	if w.docButtons[first] != nil || a.selectTab != nil || a.closeTab != nil {
		t.Fatal("removed document retained callback closures")
	}
	test.Tap(a) // stale cached widgets must not reopen a removed page.
	if w.tabs.Selected() != second || len(w.docStrip.Objects) != 1 {
		t.Fatal("removed header remains interactive")
	}
	if w.docBody.Objects[0] != second.Content {
		t.Fatal("selected document body was not restored")
	}
	var _ fyne.CanvasObject = a
}
