package ui

import (
	"context"
	"slices"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/lang"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func documentMenuWindow(t *testing.T) *Window {
	t.Helper()
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	w := &Window{App: app, Window: app.NewWindow("SQL tabs"), tabs: &documents{}, tables: make(map[*container.TabItem]*tableWorkspace), imports: make(map[*container.TabItem]*importWorkbench), designers: make(map[*container.TabItem]*tableDesigner), databases: make(map[*container.TabItem]*databaseTables)}
	w.buildDocuments()
	w.tabs.OnSelected = func(*container.TabItem) { w.syncDocuments() }
	w.Window.SetContent(container.NewStack(w.docHost, w.docTooltip.layer))
	w.Window.Resize(fyne.NewSize(800, 320))
	w.Window.Show()
	t.Cleanup(w.Window.Close)
	return w
}

func appendDocumentFixture(w *Window, title string) *container.TabItem {
	item := container.NewTabItem(title, widget.NewLabel(title))
	w.tabs.Append(item)
	w.syncDocuments()
	return item
}

func TestSQLTabMenuClosesGroupsAndDetachesHeaders(t *testing.T) {
	for _, tc := range []struct {
		name      string
		group     tabCloseGroup
		remaining []int
	}{
		{"left", tabsLeft, []int{2, 3, 4}},
		{"right", tabsRight, []int{0, 1, 2}},
		{"others", tabsOthers, []int{2}},
		{"all", tabsAll, nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			w := documentMenuWindow(t)
			items, headers := make([]*container.TabItem, 5), make([]*documentTab, 5)
			for i := range items {
				items[i] = appendDocumentFixture(w, string(rune('a'+i)))
				headers[i] = w.docButtons[items[i]]
			}
			test.TapAt(headers[2], fyne.NewPos(12, 16))
			popup, ok := w.Window.Canvas().Focused().(*widget.PopUpMenu)
			if !ok || len(popup.Items) != 4 || w.tabs.Selected() != items[2] {
				t.Fatal("SQL tab click did not select and open the close menu")
			}
			test.Tap(popup.Items[tc.group].(fyne.Tappable))
			if w.closingDocuments || w.Window.Canvas().Overlays().Top() != nil || len(w.tabs.Items) != len(tc.remaining) {
				t.Fatal("batch did not finish or closed the wrong number of tabs")
			}
			for i, original := range tc.remaining {
				if w.tabs.Items[i] != items[original] {
					t.Fatal("surviving tabs lost their order")
				}
			}
			for i, header := range headers {
				if !slices.Contains(tc.remaining, i) && (header.showMenu != nil || header.closeTab != nil || header.selectTab != nil || header.tooltip != nil) {
					t.Fatal("removed SQL header retained callbacks or tooltip")
				}
			}
			if tc.group == tabsAll {
				if w.tabs.Selected() != nil || len(w.docBody.Objects) != 0 {
					t.Fatal("closing all retained a selected document")
				}
			} else if w.tabs.Selected() != items[2] || w.docBody.Objects[0] != items[2].Content {
				t.Fatal("surviving anchor lost selection or content")
			}
		})
	}
}

func TestSQLTabMenuWaitsForConfirmationAndStopsOnCancel(t *testing.T) {
	w := documentMenuWindow(t)
	first, second, last := appendDocumentFixture(w, "first"), appendDocumentFixture(w, "second"), appendDocumentFixture(w, "last")
	ctxA, cancelA := context.WithCancel(t.Context())
	defer cancelA()
	ctxB, cancelB := context.WithCancel(t.Context())
	defer cancelB()
	a := &tableWorkspace{item: first, edits: map[int]domain.RowChange{0: {}}, cancel: cancelA}
	b := &tableWorkspace{item: second, edits: map[int]domain.RowChange{0: {}}, cancel: cancelB}
	w.tables[first], w.tables[second] = a, b
	w.closeDocumentTabs(first, tabsAll)
	if len(w.Window.Canvas().Overlays().List()) != 1 || !w.closingDocuments || len(w.tabs.Items) != 3 || a.closed || b.closed {
		t.Fatal("dirty documents closed or opened stacked confirmations")
	}
	for _, item := range w.documentTabMenu(last).Items {
		if !item.Disabled {
			t.Fatal("another batch was enabled while waiting for confirmation")
		}
	}
	clickDocumentConfirmation(t, w, true)
	if !a.closed || ctxA.Err() == nil || b.closed || ctxB.Err() != nil || len(w.tabs.Items) != 2 || len(w.Window.Canvas().Overlays().List()) != 1 {
		t.Fatal("first confirmation did not close exactly one page and advance")
	}
	clickDocumentConfirmation(t, w, false)
	if w.closingDocuments || b.closed || ctxB.Err() != nil || !slices.Equal(w.tabs.Items, []*container.TabItem{second, last}) || w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("cancel did not stop the batch and preserve remaining pages")
	}
	b.edits = nil
	w.closeDocumentTabs(second, tabsAll)
	if w.closingDocuments || len(w.tabs.Items) != 0 || !b.closed || ctxB.Err() == nil {
		t.Fatal("batch could not resume after cancellation")
	}
}

func clickDocumentConfirmation(t *testing.T, w *Window, yes bool) {
	t.Helper()
	label := lang.L("No")
	if yes {
		label = lang.L("Yes")
	}
	var found *widget.Button
	walkUpdateDialog(w.Window.Canvas().Overlays().Top(), func(object fyne.CanvasObject) {
		if button, ok := object.(*widget.Button); ok && button.Text == label {
			found = button
		}
	})
	if found == nil {
		t.Fatal("confirmation button missing")
	}
	test.Tap(found)
}

func TestSQLTabBatchClosesCatalogAndWaitsForDesignerAndImport(t *testing.T) {
	w := documentMenuWindow(t)
	catalog, designer, importer := appendDocumentFixture(w, "catalog"), appendDocumentFixture(w, "designer"), appendDocumentFixture(w, "importer")
	page := &databaseTables{item: catalog}
	d := &tableDesigner{item: designer, busy: true}
	i := &importWorkbench{item: importer, busy: true}
	w.databases[catalog], w.designers[designer], w.imports[importer] = page, d, i
	w.closeDocumentTabs(catalog, tabsAll)
	if !page.closed || d.closed || i.closed || len(w.tabs.Items) != 2 || len(w.Window.Canvas().Overlays().List()) != 1 {
		t.Fatal("catalog did not close or designer confirmation was skipped")
	}
	clickDocumentConfirmation(t, w, true)
	if !d.closed || i.closed || len(w.tabs.Items) != 1 || len(w.Window.Canvas().Overlays().List()) != 1 {
		t.Fatal("designer did not close before the import confirmation")
	}
	clickDocumentConfirmation(t, w, false)
	if w.closingDocuments || i.closed || w.imports[importer] != i {
		t.Fatal("cancel did not preserve the running import")
	}
	i.busy = false
	w.closeDocumentTabs(importer, tabsAll)
	if !i.closed || w.closingDocuments || len(w.tabs.Items) != 0 {
		t.Fatal("import did not close after the task stopped")
	}
}

func TestSQLTabBatchPersistsQueryDraftsBeforeAdvancing(t *testing.T) {
	w, profile := parityWindow(t)
	spaces := make([]*workspace, 3)
	for i := range spaces {
		spaces[i] = w.openWorkspace(profile, domain.Draft{})
		spaces[i].editor.SetText("SELECT " + string(rune('1'+i)) + ";")
	}
	waitUI(t, w)
	w.closeDocumentTabs(spaces[0].item, tabsAll)
	if !w.closingDocuments || !spaces[0].closed || spaces[1].closed {
		t.Fatal("batch did not wait for the first draft save")
	}
	waitUI(t, w)
	if w.closingDocuments || len(w.tabs.Items) != 0 || len(w.workspaces) != 0 {
		t.Fatal("query batch did not finish")
	}
	drafts, err := w.Store.Drafts(t.Context())
	if err != nil || len(drafts) != len(spaces) {
		t.Fatal("query drafts missing", err)
	}
	for _, draft := range drafts {
		if !draft.Closed || draft.Text == "" {
			t.Fatal("closed query lost its text or would reopen at startup")
		}
	}
}

func TestSQLTabBatchStopsWhenDraftCannotBeSaved(t *testing.T) {
	w, profile := parityWindow(t)
	first, second := w.openWorkspace(profile, domain.Draft{}), w.openWorkspace(profile, domain.Draft{})
	waitUI(t, w)
	if err := w.Store.Close(); err != nil {
		t.Fatal(err)
	}
	w.closeDocumentTabs(first.item, tabsAll)
	waitUI(t, w)
	if w.closingDocuments || first.closed || second.closed || len(w.tabs.Items) != 2 || first.editor.Disabled() || first.scope.Disabled() {
		t.Fatal("draft save failure did not restore the page and stop the batch")
	}
}

func TestSQLTabMenuRightClickEdgesAndStaleAnchor(t *testing.T) {
	w := documentMenuWindow(t)
	first, second := appendDocumentFixture(w, "first"), appendDocumentFixture(w, "second")
	header := w.docButtons[first]
	test.TapSecondary(header)
	popup, ok := w.Window.Canvas().Focused().(*widget.PopUpMenu)
	if !ok || w.tabs.Selected() != first {
		t.Fatal("right click did not select and open the SQL tab menu")
	}
	menu := w.documentTabMenu(first)
	if !menu.Items[0].Disabled || menu.Items[1].Disabled || menu.Items[2].Disabled || menu.Items[3].Disabled {
		t.Fatal("left edge has incorrect menu availability")
	}
	popup.TypedKey(&fyne.KeyEvent{Name: fyne.KeyEscape})
	test.Tap(test.WidgetRenderer(header).(*documentTabRenderer).close)
	for _, item := range menu.Items {
		item.Action()
	}
	test.Tap(header)
	test.TapSecondary(header)
	if !slices.Equal(w.tabs.Items, []*container.TabItem{second}) || w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("close button or stale tab menu closed a surviving document")
	}
	for i, item := range w.documentTabMenu(second).Items {
		if item.Disabled != (i != int(tabsAll)) {
			t.Fatal("single SQL tab should only enable closing all")
		}
	}
}
