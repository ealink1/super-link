package ui

import (
	"context"
	"errors"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestCreateTableUsesClickedTarget(t *testing.T) {
	w, p := parityWindow(t)
	schema := &navNode{id: "schema", kind: "schema", label: "literal.schema", profileID: p.ID, scope: "main"}
	node := &navNode{id: "schema/table", parent: schema.id, kind: "category", profileID: p.ID, scope: "main"}
	w.sidebar.nodes[schema.id] = schema
	w.selected = "unrelated"
	target := w.sidebar.tableTarget(node, p)
	if target.Scope != "main" || target.Schema != "literal.schema" || w.sidebar.createTableItem(node).Disabled {
		t.Fatal("wrong target", target)
	}
	found := false
	for _, item := range w.sidebar.nodeMenu(node).Items {
		if item.Label == "新建表" {
			found = true
		}
	}
	if !found {
		t.Fatal("missing create table action")
	}
	w.profiles[0].Config.Type = "redis"
	if !w.sidebar.createTableItem(node).Disabled {
		t.Fatal("unsupported driver enabled")
	}
}

func TestCreateTableDispatchesConfirmedStructure(t *testing.T) {
	w, p := parityWindow(t)
	node := &navNode{id: "new-table-target", kind: "database", profileID: p.ID, scope: p.Config.Database}
	target := w.sidebar.tableTarget(node, p)
	target.Name = "new_table_test"
	sql, err := sqlworkbench.CreateTableSQL(p.SQLDialect(), target, []connection.ColumnDefinition{{Name: "id", Type: "INTEGER", Nullable: "NO", Key: "PRI", Extra: "auto_increment"}})
	if err != nil {
		t.Fatal(err)
	}
	request := domain.Execution{Revision: p.Revision, Scope: target.Scope, Text: sql, Write: true, Action: "structure"}
	_, err = w.Engine.Execute(context.Background(), p.ID, request)
	var required *domain.ConfirmationRequired
	if !errors.As(err, &required) {
		t.Fatal("missing write confirmation", err)
	}
	request.Confirmation = required.Fingerprint
	completed := false
	w.sidebar.submitCreateTable(node, p, request, func(err error) {
		completed = true
		if err != nil {
			t.Error(err)
		}
	})
	waitUI(t, w)
	if !completed {
		t.Fatal("creation did not complete")
	}
}
