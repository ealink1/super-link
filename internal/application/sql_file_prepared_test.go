package application

import (
	"context"
	"errors"
	"github.com/ealink1/super-link/internal/domain"
	"os"
	"path/filepath"
	"testing"
)

func TestPreparedSQLFileChecksTwiceThenExecutesWithoutRescan(t *testing.T) {
	e, p, client := testEngine(t, false)
	path := filepath.Join(t.TempDir(), "import.sql")
	os.WriteFile(path, []byte("INSERT INTO items VALUES(1); INSERT INTO items VALUES(2);"), 0600)
	phases := []string{}
	prepared, err := e.PrepareSQLFile(context.Background(), p.ID, "main", path, p.Revision, func(progress SQLFileProgress) {
		if len(phases) == 0 || phases[len(phases)-1] != progress.Phase {
			phases = append(phases, progress.Phase)
		}
	})
	if err != nil || prepared.Info().Statements != 2 || !prepared.Info().Write || len(phases) != 2 || phases[0] != "统计语句" || phases[1] != "核对摘要" || client.calls.Load() != 0 {
		t.Fatal(phases, err)
	}
	if _, err = prepared.Execute(context.Background(), false, nil); err == nil || client.calls.Load() != 0 {
		t.Fatal("execution before explicit confirmation")
	}
	count, err := prepared.Execute(context.Background(), true, func(progress SQLFileProgress) {
		if progress.Phase != "连接数据库" && progress.Phase != "执行 SQL" {
			t.Fatal("rescanned after confirmation", progress)
		}
	})
	if err != nil || count != 2 || client.calls.Load() != 2 {
		t.Fatal(count, err)
	}
	if _, err = prepared.Execute(context.Background(), true, nil); !errors.Is(err, domain.ErrClosed) {
		t.Fatal("replayed preparation", err)
	}
}

func TestPreparedSQLFileRejectsChangeAndCancellation(t *testing.T) {
	for _, change := range []string{"file", "revision", "cancel"} {
		t.Run(change, func(t *testing.T) {
			e, p, client := testEngine(t, false)
			path := filepath.Join(t.TempDir(), "import.sql")
			os.WriteFile(path, []byte("DELETE FROM items;"), 0600)
			prepared, err := e.PrepareSQLFile(context.Background(), p.ID, "main", path, p.Revision, nil)
			if err != nil {
				t.Fatal(err)
			}
			switch change {
			case "file":
				os.WriteFile(path, []byte("DROP TABLE changed;"), 0600)
			case "revision":
				p.Name = "changed"
				if _, err = e.Profiles.Save(context.Background(), p); err != nil {
					t.Fatal(err)
				}
			case "cancel":
				prepared.Close()
			}
			if _, err = prepared.Execute(context.Background(), true, nil); err == nil || client.calls.Load() != 0 {
				t.Fatal("invalid prepared file executed", err)
			}
		})
	}
}
