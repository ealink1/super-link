package ui

import (
	"github.com/ealink1/super-link/internal/databaseexport"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestDatabaseExportFilenameUsesDatabaseAndLocalTimestamp(t *testing.T) {
	now := time.Date(2026, 10, 9, 9, 3, 21, 0, time.UTC)
	if got := databaseExportFilename("goh", now); got != "goh-20261009-170321.sql" {
		t.Fatal(got)
	}
	for _, scope := range []string{"../../bad/name", `a\b:c*?"<>|`, "..", "数据库"} {
		got := databaseExportFilename(scope, now)
		if filepath.Base(got) != got || filepath.IsAbs(got) {
			t.Fatal("database name escaped export directory", got)
		}
	}
	if got := databaseExportFilename("..", now); got != "database-20261009-170321.sql" {
		t.Fatal(got)
	}
}

func TestDatabaseExportProgressTextShowsDetails(t *testing.T) {
	text := databaseExportProgressText(databaseexport.Progress{Phase: "导出表数据", Table: "items", CompletedTables: 2, Total: 5, Rows: 1234, TableRows: 34, Bytes: 1 << 20})
	for _, want := range []string{"导出表数据", "items", "2 / 5", "1234 行", "34 行", "1.00 MiB"} {
		if !strings.Contains(text, want) {
			t.Fatal("missing progress detail", want, text)
		}
	}
}
