package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func TestCompactDocumentTabsExposeContextOnlyOnHover(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	query := container.NewTabItem("query", widget.NewLabel("query body"))
	table := container.NewTabItem("table", widget.NewLabel("table body"))
	scope := widget.NewEntry()
	scope.SetText("goh")
	profile := domain.Profile{Name: "测试连接"}
	w := &Window{
		App: app, Window: app.NewWindow("tabs"),
		tabs:       &documents{Items: []*container.TabItem{query, table}, selected: query},
		workspaces: map[*container.TabItem]*workspace{query: {profile: profile, scope: scope}},
		tables:     map[*container.TabItem]*tableWorkspace{table: {item: table, profile: profile, object: domain.Object{Name: "admin_button", Scope: "goh"}}},
	}
	defer w.Window.Close()
	w.buildDocuments()
	w.tabs.OnSelected = func(*container.TabItem) { w.syncDocuments() }
	w.syncDocuments()
	w.Window.SetContent(container.NewStack(w.docHost, w.docTooltip.layer))
	w.Window.Resize(fyne.NewSize(640, 300))
	w.Window.Show()
	q, tab := w.docButtons[query], w.docButtons[table]
	for _, header := range []*documentTab{q, tab} {
		renderer := test.WidgetRenderer(header).(*documentTabRenderer)
		if header.Size().Width != 132 || header.Size().Height != 32 {
			t.Fatalf("tab is not compact: %v", header.Size())
		}
		texts := 0
		for _, object := range renderer.Objects() {
			if _, ok := object.(*canvas.Text); ok {
				texts++
			}
		}
		if texts != 1 {
			t.Fatal("visible subtitle remains in the header")
		}
	}
	if w.docTooltip.box.Visible() || w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("tooltip is visible before hovering")
	}
	position := app.Driver().AbsolutePositionForObject(tab).Add(fyne.NewPos(10, 16))
	test.MoveMouse(w.Window.Canvas(), position)
	if !w.docTooltip.box.Visible() || w.docTooltip.label.Text != "admin_button\n连接：测试连接\n数据库：goh" {
		t.Fatalf("hover does not show full context: %q", w.docTooltip.label.Text)
	}
	if w.Window.Canvas().Overlays().Top() != nil || w.docTooltip.layer.MinSize() != (fyne.Size{}) {
		t.Fatal("tooltip captures canvas events or enlarges the window minimum")
	}
	// The first click while the tooltip is visible must select the tab.
	test.TapCanvas(w.Window.Canvas(), position)
	if w.tabs.Selected() != table || w.docTooltip.box.Visible() {
		t.Fatal("tooltip blocked tab selection or survived selection")
	}
	tab.MouseIn(nil)
	tab.MouseOut()
	if w.docTooltip.active != nil || w.docTooltip.box.Visible() || w.docTooltip.label.Text != "" {
		t.Fatal("mouse-out retained a tooltip or document reference")
	}
	// Database changes update the next hover rather than keeping stale context.
	scope.SetText("updated_db")
	w.syncDocuments()
	q.MouseIn(nil)
	if !strings.Contains(w.docTooltip.label.Text, "数据库：updated_db") {
		t.Fatal("query tooltip uses an obsolete database")
	}
	w.tabs.Remove(query)
	delete(w.workspaces, query)
	w.syncDocuments()
	q.MouseIn(nil)
	if w.docTooltip.active != nil || w.docTooltip.box.Visible() || q.tooltip != nil {
		t.Fatal("closed tab reopened its tooltip or retained the shared layer")
	}
	tab.MouseIn(nil)
	app.Settings().SetTheme(Theme{Dark: true})
	tab.MouseOut()
	tab.MouseIn(nil)
	if w.docTooltip.background.FillColor != theme.OverlayBackgroundColor() {
		t.Fatal("tooltip did not adopt the current theme")
	}
	closePosition := app.Driver().AbsolutePositionForObject(tab).Add(fyne.NewPos(tab.Size().Width-17, 16))
	test.TapCanvas(w.Window.Canvas(), closePosition)
	if w.docButtons[table] != nil || tab.selectTab != nil || tab.closeTab != nil || w.docTooltip.active != nil {
		t.Fatal("hover prevented closing a tab or retained its callbacks")
	}
}

func TestDocumentTooltipFitsRightEdgeAndDoesNotCoverDialogs(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	tip := newDocumentTooltip()
	tab := newDocumentTab(strings.Repeat("长名称", 30), "连接：测试\n数据库：db", false, nil, nil)
	tab.tooltip = tip
	tab.Resize(tab.MinSize())
	tab.Move(fyne.NewPos(250, 0))
	window := app.NewWindow("tooltip bounds")
	defer window.Close()
	window.SetContent(container.NewStack(container.NewWithoutLayout(tab), tip.layer))
	window.Resize(fyne.NewSize(400, 400))
	window.Show()
	tab.MouseIn(nil)
	if !tip.box.Visible() || tip.label.Text != tab.title+"\n"+tab.subtitle {
		t.Fatal("long title was lost from the tooltip")
	}
	if tip.box.Position().X < 0 || tip.box.Position().X+tip.box.Size().Width > tip.layer.Size().Width || tip.box.Position().Y+tip.box.Size().Height > tip.layer.Size().Height {
		t.Fatalf("tooltip extends beyond the document area: %v %v", tip.box.Position(), tip.box.Size())
	}
	tab.MouseOut()
	modal := widget.NewModalPopUp(widget.NewLabel("dialog"), window.Canvas())
	modal.Show()
	tab.MouseIn(nil)
	if tip.box.Visible() || tip.active != nil {
		t.Fatal("tooltip appeared over a dialog")
	}
	modal.Hide()
	tab.MouseIn(nil)
	test.WidgetRenderer(tab).Destroy()
	if tip.box.Visible() || tip.active != nil {
		t.Fatal("destroyed tab retained the tooltip")
	}
}
