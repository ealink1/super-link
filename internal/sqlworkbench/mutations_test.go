package sqlworkbench

import (
	"reflect"
	"strings"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestChangesUseBoundValuesAndPrimaryKeyLocator(t *testing.T) {
	original := map[string]any{"id": int64(1), "name": "before", "optional": nil}
	changes := domain.TableChanges{Object: domain.Object{Schema: "a.b", Name: "items"}, Rows: []domain.RowChange{{Kind: "update", Original: original, Values: map[string]any{"name": "'; DELETE FROM items; --"}}, {Kind: "delete", Original: original}}}
	statements, err := BuildChanges("postgres", changes, testInfo())
	if err != nil {
		t.Fatal(err)
	}
	if len(statements) != 2 || strings.Contains(statements[0].SQL, "DELETE FROM items") {
		t.Fatal(statements)
	}
	if statements[0].SQL != `UPDATE "a.b"."items" SET "name" = $1 WHERE "id" = $2` {
		t.Fatal(statements[0].SQL)
	}
	if !reflect.DeepEqual(statements[0].Args, []any{"'; DELETE FROM items; --", int64(1)}) || !statements[0].RequireOne {
		t.Fatal(statements[0])
	}
	if statements[1].SQL != `DELETE FROM "a.b"."items" WHERE "id" = $1` || !reflect.DeepEqual(statements[1].Args, []any{int64(1)}) || !statements[1].RequireOne {
		t.Fatal(statements[1])
	}
}

func TestChangesUseAllOriginalCompositePrimaryKeyFields(t *testing.T) {
	info := domain.TableInfo{Columns: []connection.ColumnDefinition{
		{Name: "tenant_id", Key: "PRI", Type: "INT"},
		{Name: "name", Type: "VARCHAR(255)"},
		{Name: "id", Key: "PRI", Type: "BIGINT"},
		{Name: "updated_at", Type: "DATETIME"},
	}}
	original := map[string]any{"tenant_id": "7", "id": "26", "name": "before", "updated_at": "2026-06-11T10:00:18+08:00"}
	changes := domain.TableChanges{Object: domain.Object{Scope: "catalog", Name: "items"}, Rows: []domain.RowChange{
		{Kind: "update", Original: original, Values: map[string]any{"id": int64(27), "name": "after"}},
		{Kind: "delete", Original: original},
	}}
	statements, err := BuildChanges("mysql", changes, info)
	if err != nil {
		t.Fatal(err)
	}
	if statements[0].SQL != "UPDATE `catalog`.`items` SET `name` = ?, `id` = ? WHERE `tenant_id` = ? AND `id` = ?" ||
		!reflect.DeepEqual(statements[0].Args, []any{"after", int64(27), int64(7), int64(26)}) {
		t.Fatal(statements[0])
	}
	if statements[1].SQL != "DELETE FROM `catalog`.`items` WHERE `tenant_id` = ? AND `id` = ?" ||
		!reflect.DeepEqual(statements[1].Args, []any{int64(7), int64(26)}) {
		t.Fatal(statements[1])
	}
	for _, field := range []string{"tenant_id", "id"} {
		incomplete := make(map[string]any)
		for key, value := range original {
			if key != field {
				incomplete[key] = value
			}
		}
		for _, kind := range []string{"update", "delete"} {
			changes.Rows = []domain.RowChange{{Kind: kind, Original: incomplete, Values: map[string]any{"name": "after"}}}
			if _, err := BuildChanges("mysql", changes, info); err == nil {
				t.Fatalf("accepted %s with missing primary key %s", kind, field)
			}
		}
	}
}

func TestChangesRejectUnknownFieldsMissingLocatorAndNoOps(t *testing.T) {
	info := testInfo()
	base := domain.TableChanges{Object: domain.Object{Name: "items"}}
	for _, row := range []domain.RowChange{{Kind: "update", Values: map[string]any{"name": "x"}}, {Kind: "insert", Values: map[string]any{"unknown": "x"}}, {Kind: "insert"}, {Kind: "drop"}} {
		base.Rows = []domain.RowChange{row}
		if _, err := BuildChanges("sqlite", base, info); err == nil {
			t.Fatal("unsafe row accepted", row)
		}
	}
	base.Rows = []domain.RowChange{{Kind: "delete", Original: map[string]any{"id": 1, "name": "x", "optional": nil}}}
	info.Columns[0].Key = ""
	if _, err := BuildChanges("sqlite", base, info); err == nil {
		t.Fatal("keyless delete accepted")
	}
	base.Rows = []domain.RowChange{{Kind: "insert", Values: map[string]any{"name": "x"}}}
	statements, err := BuildChanges("sqlite", base, info)
	if err != nil || statements[0].SQL != `INSERT INTO "items" ("name") VALUES (?)` {
		t.Fatal(statements, err)
	}
}
