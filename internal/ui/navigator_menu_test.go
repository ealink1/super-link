package ui

import (
	"strings"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestObjectQueryUsesClickedConnectionAndLiteralSchema(t *testing.T) {
	w, p := parityWindow(t)
	node := &navNode{id: "table", kind: "object", profileID: p.ID, scope: "main", object: domain.Object{Name: "report.daily", Schema: "public", Scope: "main", Kind: "table"}}
	w.selected = "different-connection"
	w.sidebar.queryForNode(node)
	waitUI(t, w)
	s := w.workspaces[w.tabs.Selected()]
	if s == nil || s.profile.ID != p.ID || s.scope.Text != "main" || s.schemaName != "public" || s.editor.Text != `SELECT * FROM "public"."report.daily" LIMIT 100;` {
		t.Fatal("object action followed a mutable selection or split a literal table name")
	}
	menu := w.sidebar.nodeMenu(node)
	labels := map[string]bool{}
	for _, item := range menu.Items {
		labels[item.Label] = true
	}
	for _, label := range []string{"查看数据", "设计表", "新建查询", "DDL", "复制名称", "复制结构", "复制 INSERT 模板", "导出数据"} {
		if !labels[label] {
			t.Fatal("missing working object action", label)
		}
	}
}

func TestInsertTemplateQuotesMetadataAndExcludesGeneratedColumns(t *testing.T) {
	info := domain.TableInfo{Columns: []connection.ColumnDefinition{{Name: "id", Extra: "auto_increment"}, {Name: "computed", Extra: "generated"}, {Name: "value.name"}}}
	text, err := insertTemplate("postgres", domain.Object{Name: `a"b`, Schema: "public"}, info)
	if err != nil || !strings.Contains(text, `"public"."a""b" ("value.name")`) || !strings.Contains(text, ":value_1") || strings.Contains(text, "computed") {
		t.Fatal("INSERT template changed identifiers or included generated values", err)
	}
}

func TestCreateDatabaseMenuUsesClickedProfileAndReadOnlyPolicy(t *testing.T) {
	w, p := parityWindow(t)
	p.Config.Type = "mysql"
	w.profiles = []domain.Profile{p}
	w.selected = "another-connection"
	node := &navNode{id: p.ID, kind: "connection", profileID: p.ID}
	menu := w.sidebar.nodeMenu(node)
	if len(menu.Items) < 2 || menu.Items[1].Label != "新建库" || menu.Items[1].Disabled {
		t.Fatal("connection menu did not enable database creation for clicked MySQL profile")
	}
	w.profiles[0].ReadOnly = true
	if w.sidebar.createDatabaseItem(node).Disabled {
		t.Fatal("read-only connection hides the explanation and editing entry")
	}
	w.profiles[0].ReadOnly = false
	w.profiles[0].Config.Type = "sqlite"
	if !w.sidebar.createDatabaseItem(node).Disabled {
		t.Fatal("SQLite connection permits CREATE DATABASE")
	}
	node.kind = "scope"
	for _, item := range w.sidebar.nodeMenu(node).Items {
		if item.Label == "新建库" {
			t.Fatal("database node exposes connection-only action")
		}
	}
}
