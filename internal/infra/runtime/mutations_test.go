package runtime

import (
	"context"
	"database/sql"
	"errors"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"github.com/ealink1/super-link/internal/upstream/connection"
	"github.com/ealink1/super-link/internal/upstream/db"
	_ "modernc.org/sqlite"
)

func TestAtomicChangesCommitRollbackAndKeepBoundValues(t *testing.T) {
	ctx := context.Background()
	database, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		t.Fatal(err)
	}
	defer database.Close()
	conn, err := database.Conn(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	if _, err = conn.ExecContext(ctx, "CREATE TABLE items(id INTEGER PRIMARY KEY, name TEXT UNIQUE)"); err != nil {
		t.Fatal(err)
	}
	d, _ := domain.Resolve("sqlite")
	client := &databaseClient{session: db.NewSQLConnStatementExecer(conn), descriptor: d}
	unsafeValue := "中文\\'; DROP TABLE items; --"
	rows, err := client.ApplyStatements(ctx, []domain.Statement{{SQL: "INSERT INTO items VALUES (?,?)", Args: []any{1, unsafeValue}, RequireOne: true}})
	if err != nil || rows != 1 {
		t.Fatal(rows, err)
	}
	var name string
	if err = conn.QueryRowContext(ctx, "SELECT name FROM items WHERE id=1").Scan(&name); err != nil || name != unsafeValue {
		t.Fatal(name, err)
	}
	// The second conflict must also undo the first successful statement.
	_, err = client.ApplyStatements(ctx, []domain.Statement{{SQL: "UPDATE items SET name=? WHERE id=?", Args: []any{"modified", 1}, RequireOne: true}, {SQL: "DELETE FROM items WHERE id=?", Args: []any{999}, RequireOne: true}})
	if err == nil {
		t.Fatal("missing optimistic conflict")
	}
	var rolledBack *domain.RolledBackError
	if !errors.As(err, &rolledBack) || errors.Is(err, domain.ErrUnknownOutcome) {
		t.Fatal("confirmed rollback was not distinguished from uncertain writes", err)
	}
	if err = conn.QueryRowContext(ctx, "SELECT name FROM items WHERE id=1").Scan(&name); err != nil || name != unsafeValue {
		t.Fatal("partial batch committed", name, err)
	}
	_, err = client.ApplyStatements(ctx, []domain.Statement{{SQL: "INSERT INTO items VALUES (?,?)", Args: []any{2, "first"}, RequireOne: true}, {SQL: "INSERT INTO items VALUES (?,?)", Args: []any{2, "duplicate"}, RequireOne: true}})
	if err == nil {
		t.Fatal("duplicate key accepted")
	}
	var count int
	if err = conn.QueryRowContext(ctx, "SELECT COUNT(*) FROM items").Scan(&count); err != nil || count != 1 {
		t.Fatal("constraint failure left partial insert", count, err)
	}
}

func TestPrimaryKeyChangesIgnoreStaleNonKeyValuesAndKeepOtherRows(t *testing.T) {
	ctx := context.Background()
	database, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		t.Fatal(err)
	}
	defer database.Close()
	conn, err := database.Conn(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	if _, err = conn.ExecContext(ctx, `CREATE TABLE items(tenant_id INTEGER, id INTEGER, number TEXT, updated_at TEXT, PRIMARY KEY(tenant_id,id));
		INSERT INTO items VALUES (1,26,'','2026-06-11 10:00:18'), (2,26,'other tenant','unchanged'), (1,27,'other id','unchanged')`); err != nil {
		t.Fatal(err)
	}
	descriptor, _ := domain.Resolve("sqlite")
	client := &databaseClient{session: db.NewSQLConnStatementExecer(conn), descriptor: descriptor}
	info := domain.TableInfo{Columns: []connection.ColumnDefinition{
		{Name: "tenant_id", Type: "INTEGER", Key: "PRI"},
		{Name: "id", Type: "INTEGER", Key: "PRI"},
		{Name: "number", Type: "TEXT"},
		{Name: "updated_at", Type: "DATETIME"},
	}}
	original := map[string]any{"tenant_id": int64(1), "id": int64(26), "number": "", "updated_at": "2026-06-11T10:00:18+08:00"}
	for _, kind := range []string{"update", "delete"} {
		changes := domain.TableChanges{Object: domain.Object{Name: "items"}, Rows: []domain.RowChange{{Kind: kind, Original: original}}}
		if kind == "update" {
			changes.Rows[0].Values = map[string]any{"number": "32"}
		}
		statements, err := sqlworkbench.BuildChanges("sqlite", changes, info)
		if err != nil {
			t.Fatal(err)
		}
		rows, err := client.ApplyStatements(ctx, statements)
		if err != nil || rows != 1 {
			t.Fatalf("%s affected %d rows: %v", kind, rows, err)
		}
		if kind == "update" {
			var number, updatedAt string
			if err := conn.QueryRowContext(ctx, "SELECT number,updated_at FROM items WHERE tenant_id=1 AND id=26").Scan(&number, &updatedAt); err != nil {
				t.Fatal(err)
			}
			if number != "32" || updatedAt != "2026-06-11 10:00:18" {
				t.Fatal("update changed an unedited field")
			}
		}
		var others int
		if err := conn.QueryRowContext(ctx, "SELECT COUNT(*) FROM items WHERE (tenant_id=2 AND id=26 AND number='other tenant') OR (tenant_id=1 AND id=27 AND number='other id')").Scan(&others); err != nil || others != 2 {
			t.Fatalf("%s did not preserve other composite keys: %v", kind, err)
		}
	}
	var remaining int
	if err := conn.QueryRowContext(ctx, "SELECT COUNT(*) FROM items WHERE tenant_id=1 AND id=26").Scan(&remaining); err != nil || remaining != 0 {
		t.Fatal("delete did not remove the primary key", err)
	}
}
