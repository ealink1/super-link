package databaseexport

import (
	"context"
	"errors"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type fixture struct {
	fail    bool
	request domain.Execution
}

func (*fixture) Objects(context.Context, string, string) ([]domain.Object, error) {
	return []domain.Object{{Name: "items", Kind: "table"}, {Name: "summary", Kind: "view"}}, nil
}
func (*fixture) TableInfo(context.Context, string, string, string) (domain.TableInfo, error) {
	return domain.TableInfo{DDL: "CREATE TABLE `items` (`name` TEXT)", Columns: []connection.ColumnDefinition{{Name: "name"}}}, nil
}
func (f *fixture) StreamQuery(ctx context.Context, _ string, request domain.Execution, consumer domain.RowConsumer) error {
	f.request = request
	if err := consumer.SetColumns([]string{"name"}); err != nil {
		return err
	}
	if err := consumer.ConsumeRowValues([]any{"O'Reilly"}); err != nil {
		return err
	}
	if f.fail {
		return errors.New("transport failed")
	}
	return ctx.Err()
}
func TestDatabaseExportWritesStructureAndStreamsRows(t *testing.T) {
	source := &fixture{}
	p := domain.Profile{ID: "p", Revision: 3, Config: connection.ConnectionConfig{Type: "mysql"}}
	path := filepath.Join(t.TempDir(), "dump.sql")
	result, err := Save(context.Background(), source, p, "demo", path)
	if err != nil || result.Tables != 1 || result.Rows != 1 {
		t.Fatal(result, err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	for _, want := range []string{"USE `demo`;", "CREATE TABLE `items`", "INSERT INTO `demo`.`items`", "FOREIGN_KEY_CHECKS=@SUPERLINK_OLD_FK"} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in SQL export", want)
		}
	}
	if source.request.Revision != p.Revision || source.request.Scope != "demo" || source.request.Write || !strings.Contains(source.request.Text, "SELECT `name`") {
		t.Fatal("stream lost target or revision", source.request)
	}
}
func TestFailedDatabaseExportPreservesExistingFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "dump.sql")
	if err := os.WriteFile(path, []byte("original"), 0600); err != nil {
		t.Fatal(err)
	}
	p := domain.Profile{Config: connection.ConnectionConfig{Type: "mysql"}}
	for _, fail := range []bool{true, false} {
		if _, err := Save(context.Background(), &fixture{fail: fail}, p, "demo", path); err == nil {
			t.Fatal("export replaced existing destination")
		}
	}
	raw, _ := os.ReadFile(path)
	if string(raw) != "original" {
		t.Fatal("existing file changed")
	}
	entries, _ := os.ReadDir(dir)
	if len(entries) != 1 {
		t.Fatal("temporary export file leaked")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Save(ctx, &fixture{}, p, "demo", filepath.Join(dir, "cancelled.sql")); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

func TestDatabaseExportStructureOnlyNeverReadsRows(t *testing.T) {
	source := &fixture{fail: true}
	p := domain.Profile{Config: connection.ConnectionConfig{Type: "mysql"}}
	path := filepath.Join(t.TempDir(), "structure.sql")
	result, err := SaveWithOptions(context.Background(), source, p, "demo", path, false)
	if err != nil || result.Tables != 1 || result.Rows != 0 {
		t.Fatal(result, err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), "CREATE TABLE") || strings.Contains(string(raw), "INSERT INTO") || source.request.Text != "" {
		t.Fatal("structure export fetched or emitted table data")
	}
}

func TestDatabaseExportProgressReportsStagesRowsAndPublishedCompletion(t *testing.T) {
	for _, data := range []bool{false, true} {
		p := domain.Profile{Config: connection.ConnectionConfig{Type: "mysql"}}
		var updates []Progress
		path := filepath.Join(t.TempDir(), "dump.sql")
		_, err := SaveWithProgress(context.Background(), &fixture{}, p, "demo", path, data, func(progress Progress) { updates = append(updates, progress) })
		if err != nil {
			t.Fatal(err)
		}
		previous := float64(0)
		sawTable := false
		for _, progress := range updates {
			if progress.Fraction() < previous {
				t.Fatal("progress regressed")
			}
			previous = progress.Fraction()
			if progress.Table == "items" {
				sawTable = true
			}
			if !progress.Done && progress.Fraction() == 1 {
				t.Fatal("reported complete before publication")
			}
		}
		last := updates[len(updates)-1]
		if !sawTable || !last.Done || last.CompletedTables != 1 || last.Total != 1 || last.Bytes == 0 || last.Fraction() != 1 {
			t.Fatal("incomplete progress", last)
		}
		wantRows := int64(0)
		if data {
			wantRows = 1
		}
		if last.Rows != wantRows {
			t.Fatal("wrong exported row count", last)
		}
	}
	var done bool
	_, err := SaveWithProgress(context.Background(), &fixture{fail: true}, domain.Profile{Config: connection.ConnectionConfig{Type: "mysql"}}, "demo", filepath.Join(t.TempDir(), "failed.sql"), true, func(p Progress) { done = done || p.Done })
	if err == nil || done {
		t.Fatal("failed export reported complete")
	}
}
