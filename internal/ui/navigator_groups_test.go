package ui

import (
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
)

func TestNavigatorGroupsConnectionsAndPreservesChildren(t *testing.T) {
	w, p := parityWindow(t)
	n := w.sidebar
	p.Group = "ysy"
	w.profiles = []domain.Profile{p}
	for _, name := range []string{"ysy-local-104", "online-jz", "aliyun"} {
		other := p
		other.ID = name
		other.Name = name
		if name == "aliyun" {
			other.Group = ""
		}
		w.profiles = append(w.profiles, other)
	}
	w.filter()
	group := n.nodes["connection-group:ysy"]
	if group == nil || group.count != 3 || len(group.children) != 3 || len(n.roots) != 2 {
		t.Fatal("wrong grouped roots", n.roots, group)
	}
	if n.roots[0] != group.id || n.roots[1] != "connection:aliyun" {
		t.Fatal("ungrouped connection was wrapped", n.roots)
	}
	connection := n.nodes["connection:"+p.ID]
	if !n.tree.IsBranchOpen(group.id) {
		t.Fatal("new group hides its connections")
	}
	if connection.parent != group.id {
		t.Fatal("missing parent for grouped connection")
	}
	connection.loaded = true
	leaf := &navNode{id: connection.id + "/test", kind: "database", parent: connection.id, profileID: p.ID}
	n.nodes[leaf.id] = leaf
	connection.children = []string{leaf.id}
	n.syncProfiles()
	if n.nodes[leaf.id] != leaf {
		t.Fatal("group refresh discarded connection subtree")
	}
	n.tree.OpenBranch(group.id)
	if len(group.children) != 3 || group.busy {
		t.Fatal("opening group tried to load database objects")
	}
	n.selected = connection.id
	n.tree.CloseBranch(group.id)
	n.locate()
	if !n.tree.IsBranchOpen(group.id) {
		t.Fatal("locate did not open containing group")
	}
	row := newTreeRow(n)
	row.bind(group)
	if !row.countView.Visible() || row.count.Text != "3" || row.status.Visible() {
		t.Fatal("group badge or status incorrect")
	}
	row.bind(connection)
	if row.countView.Visible() {
		t.Fatal("connection reused group count badge")
	}
	if len(n.nodeMenu(group).Items) != 2 {
		t.Fatal("group exposes connection operations")
	}
	w.profiles[0].Group = ""
	w.profiles[1].Group = ""
	w.profiles[2].Group = ""
	w.filter()
	if n.nodes[group.id] != nil || len(n.roots) != 4 || n.nodes[leaf.id] != leaf {
		t.Fatal("removing group removed connections or left stale folder")
	}
}

func TestNavigatorGroupsSearchAndScreenshot(t *testing.T) {
	w, p := parityWindow(t)
	p.Group = "ysy"
	w.profiles = []domain.Profile{p}
	for _, name := range []string{"ysy-local-104", "online-jz", "online-jay", "aliyun"} {
		other := p
		other.ID = name
		other.Name = name
		if name == "online-jay" || name == "aliyun" {
			other.Group = ""
		}
		w.profiles = append(w.profiles, other)
	}
	w.filter()
	w.search.SetText("ysy")
	group := w.sidebar.nodes["connection-group:ysy"]
	if group == nil || group.count != 3 || len(w.sidebar.roots) != 1 {
		t.Fatal("group search did not retain members")
	}
	w.search.SetText("")
	w.sidebar.tree.OpenBranch(group.id)
	w.Window.Resize(fyne.NewSize(1000, 680))
	test.WidgetRenderer(w.sidebar.tree).Refresh()
	captureConnectionIndicators(t, w.Window, "navigator-groups.png")
}
