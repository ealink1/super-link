package application

import (
	"context"
	"errors"
	"github.com/ealink1/super-link/internal/domain"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSQLFileExecutesLargeStatementsWithoutWholeFileLimit(t *testing.T) {
	e, p, client := testEngine(t, false)
	path := filepath.Join(t.TempDir(), "large.sql")
	if err := os.WriteFile(path, []byte("SELECT '"+strings.Repeat("x", 2<<20)+"';SELECT 2;"), 0600); err != nil {
		t.Fatal(err)
	}
	info, err := e.InspectSQLFile(context.Background(), p.ID, path, p.Revision)
	if err != nil || info.Statements != 2 || info.Write {
		t.Fatal(info, err)
	}
	var progress SQLFileProgress
	var checked, executing bool
	count, err := e.ExecuteSQLFile(context.Background(), p.ID, "main", path, p.Revision, info, "", func(p SQLFileProgress) {
		progress = p
		if p.Phase == "核对文件" && p.Checked == 2 && p.Bytes == info.Bytes {
			checked = true
		}
		if p.Phase == "执行 SQL" && p.Current == 1 && p.Completed == 0 {
			executing = true
		}
	})
	if err != nil || count != 2 || client.calls.Load() != 2 || progress.Completed != 2 || !checked || !executing || client.closed.Load() == 0 {
		t.Fatal(count, progress, err)
	}
}
func TestSQLFileWritesRequireConfirmationAndRejectChangedFile(t *testing.T) {
	e, p, client := testEngine(t, false)
	path := filepath.Join(t.TempDir(), "write.sql")
	os.WriteFile(path, []byte("INSERT INTO items VALUES(1); INSERT INTO items VALUES(2);"), 0600)
	info, err := e.InspectSQLFile(context.Background(), p.ID, path, p.Revision)
	if err != nil {
		t.Fatal(err)
	}
	_, err = e.ExecuteSQLFile(context.Background(), p.ID, "main", path, p.Revision, info, "", nil)
	var required *domain.ConfirmationRequired
	if !errors.As(err, &required) || client.calls.Load() != 0 {
		t.Fatal("file bypassed confirmation", err)
	}
	count, err := e.ExecuteSQLFile(context.Background(), p.ID, "main", path, p.Revision, info, required.Fingerprint, nil)
	if err != nil || count != 2 {
		t.Fatal(count, err)
	}
	os.WriteFile(path, []byte("INSERT INTO items VALUES(3);"), 0600)
	if _, err = e.ExecuteSQLFile(context.Background(), p.ID, "main", path, p.Revision, info, "", nil); err == nil {
		t.Fatal("changed file accepted")
	}
	ro, rp, _ := testEngine(t, true)
	if _, err = ro.InspectSQLFile(context.Background(), rp.ID, path, rp.Revision); !errors.Is(err, domain.ErrReadOnly) {
		t.Fatal("readonly file accepted", err)
	}
}

func TestSQLFileCancellationStopsAfterCompletedStatementAndClosesSession(t *testing.T) {
	e, p, client := testEngine(t, false)
	path := filepath.Join(t.TempDir(), "cancel.sql")
	if err := os.WriteFile(path, []byte("SELECT 1; SELECT 2; SELECT 3;"), 0600); err != nil {
		t.Fatal(err)
	}
	info, err := e.InspectSQLFile(context.Background(), p.ID, path, p.Revision)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	count, err := e.ExecuteSQLFile(ctx, p.ID, "main", path, p.Revision, info, "", func(p SQLFileProgress) {
		if p.Completed == 1 {
			cancel()
		}
	})
	if !errors.Is(err, context.Canceled) || count != 1 || client.calls.Load() != 1 || client.closed.Load() == 0 {
		t.Fatal("cancellation did not stop and close session", count, err)
	}
}
