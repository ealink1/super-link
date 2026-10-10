package ui

import (
	"github.com/ealink1/super-link/internal/application"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestConnectionGroupsMoveSelectionAndPersistEmptyGroups(t *testing.T) {
	w, p := parityWindow(t)
	other := p
	other.ID, other.Name, other.Revision = "", "第二连接", 0
	other, err := w.Profiles.Save(t.Context(), other)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.Profiles.AddGroup(t.Context(), "测试分组"); err != nil {
		t.Fatal(err)
	}
	w.selected = ""
	d := newConnectionGroups(w)
	d.show()
	waitUI(t, w)
	if len(d.rows) != 2 || len(d.groups) != 1 {
		t.Fatal("missing connections or persistent group")
	}
	d.selected[other.ID] = true
	d.destination.SetSelected("测试分组")
	d.moveSelected()
	waitUI(t, w)
	stored, err := w.Profiles.Get(t.Context(), other.ID)
	if err != nil || stored.Group != "测试分组" {
		t.Fatal("selected connection not moved", stored.Group, err)
	}
	original, err := w.Profiles.Get(t.Context(), p.ID)
	if err != nil || original.Group != p.Group {
		t.Fatal("different connection changed", err)
	}
	if stored.CreatedAt != other.CreatedAt {
		t.Fatal("move changed creation time")
	}
	reopened := newConnectionGroups(w)
	reopened.show()
	waitUI(t, w)
	if len(reopened.groups) != 1 {
		t.Fatal("group disappeared after reopening")
	}
}

func TestConnectionGroupsSortAndEmptyState(t *testing.T) {
	w, p := parityWindow(t)
	other := p
	other.ID, other.Name, other.Revision = "", "A连接", 0
	if _, err := w.Profiles.Save(t.Context(), other); err != nil {
		t.Fatal(err)
	}
	d := newConnectionGroups(w)
	d.show()
	waitUI(t, w)
	d.order.SetSelected("连接名称 ↑")
	if d.rows[0].Name != "A连接" {
		t.Fatal("ascending sort failed")
	}
	d.order.SetSelected("连接名称 ↓")
	if d.rows[0].Name != p.Name {
		t.Fatal("descending sort failed")
	}
	d.active = "空分组"
	d.filter()
	if !d.empty.Visible() || !d.move.Disabled() {
		t.Fatal("missing empty group state")
	}
}

func TestConnectionGroupsReferenceCapture(t *testing.T) {
	w, p := parityWindow(t)
	for _, name := range []string{"online-jay", "aliyun", "ysy-local-104", "online-jz"} {
		other := p
		other.ID, other.Name, other.Revision = "", name, 0
		if _, err := w.Profiles.Save(t.Context(), other); err != nil {
			t.Fatal(err)
		}
	}
	w.Window.Resize(fyne.NewSize(1470, 862))
	d := newConnectionGroups(w)
	d.show()
	waitUI(t, w)
	captureConnectionIndicators(t, w.Window, "connection-groups.png")
}

func TestConnectionGroupsTableControls(t *testing.T) {
	w, p := parityWindow(t)
	d := newConnectionGroups(w)
	d.show()
	waitUI(t, w)
	header := d.newCell().(*connectionGroupCell)
	d.table.UpdateHeader(widget.TableCellID{Row: -1, Col: 0}, header)
	test.Tap(header.check)
	if !d.selected[p.ID] {
		t.Fatal("select all did not select connection")
	}
	cell := d.newCell().(*connectionGroupCell)
	d.table.UpdateCell(widget.TableCellID{Row: 0, Col: 1}, cell)
	if !cell.label.Visible() || cell.label.Text != p.Name {
		t.Fatal("connection cell did not bind")
	}
	d.table.UpdateCell(widget.TableCellID{Row: 0, Col: 5}, cell)
	if !cell.actions.Visible() {
		t.Fatal("row actions hidden")
	}
	test.Tap(cell.remove)
	var confirm *widget.Button
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
			if button, ok := object.(*widget.Button); ok && (button.Text == "Yes" || button.Text == "是") {
				confirm = button
			}
		})
	}
	if confirm == nil {
		t.Fatal("missing deletion confirmation")
	}
	test.Tap(confirm)
	waitUI(t, w)
	if _, err := w.Profiles.Get(t.Context(), p.ID); err == nil {
		t.Fatal("connection was not deleted")
	}
	if len(d.rows) != 0 || !d.empty.Visible() {
		t.Fatal("deleted connection remains in manager")
	}
}

func TestConnectionGroupsSearchStatusAndHiddenSelection(t *testing.T) {
	w, p := parityWindow(t)
	other := p
	other.ID, other.Name, other.Revision = "", "Alpha", 0
	other.Config.Host = "db.example.test"
	other, err := w.Profiles.Save(t.Context(), other)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.Profiles.AddGroup(t.Context(), "生产环境"); err != nil {
		t.Fatal(err)
	}
	d := newConnectionGroups(w)
	d.show()
	waitUI(t, w)
	d.selected[p.ID] = true
	d.search.SetText("DB.EXAMPLE")
	if len(d.rows) != 1 || d.rows[0].ID != other.ID {
		t.Fatal("address search failed")
	}
	d.search.SetText("alpha")
	if len(d.rows) != 1 {
		t.Fatal("name search failed")
	}
	d.statuses[other.ID] = application.ConnectionConnected
	cell := d.newCell().(*connectionGroupCell)
	d.table.UpdateCell(widget.TableCellID{Row: 0, Col: 3}, cell)
	if !cell.statusView.Visible() || cell.statusText.Text != "在线" {
		t.Fatal("missing actual connection status")
	}
	d.search.SetText("no matches")
	if !d.empty.Visible() {
		t.Fatal("missing search empty state")
	}
	d.destination.SetSelected("生产环境")
	if d.move.Disabled() {
		t.Fatal("search lost existing selection")
	}
	d.moveSelected()
	waitUI(t, w)
	stored, err := w.Profiles.Get(t.Context(), p.ID)
	if err != nil || stored.Group != "生产环境" {
		t.Fatal("hidden selection was not moved", err)
	}
	unchanged, err := w.Profiles.Get(t.Context(), other.ID)
	if err != nil || unchanged.Group != "" {
		t.Fatal("search match moved without selection", err)
	}
}
