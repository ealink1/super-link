package ui

import (
	"context"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type sqlFileUIClient struct {
	parityFixture
	requests []domain.Execution
}

func (c *sqlFileUIClient) Execute(_ context.Context, r domain.Execution) ([]domain.Result, error) {
	c.requests = append(c.requests, r)
	return []domain.Result{{}}, nil
}

func TestSQLFileExecutionUsesClickedScopeAndWaitsForConfirmation(t *testing.T) {
	for _, sql := range []string{"SELECT 1;", "DELETE FROM items;"} {
		t.Run(sql, func(t *testing.T) {
			w, p := parityWindow(t)
			w.Engine.Disconnect(context.Background(), p.ID)
			client := &sqlFileUIClient{}
			w.Engine.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return client, nil }
			path := filepath.Join(t.TempDir(), "script.sql")
			os.WriteFile(path, []byte(sql), 0600)
			w.selected = "another-connection"
			w.sidebar.loadSQLFile(p, "main", path)
			overlay := w.Window.Canvas().Overlays().Top()
			if overlay == nil {
				t.Fatal("no immediate dialog")
			}
			var confirm *widget.Button
			walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
				if b, ok := object.(*widget.Button); ok && b.Text == "确认执行" {
					confirm = b
				}
			})
			if confirm == nil || !confirm.Disabled() {
				t.Fatal("confirmation enabled before checks")
			}
			waitUI(t, w)
			if w.Window.Canvas().Overlays().Top() != overlay || confirm.Disabled() || len(client.requests) != 0 {
				t.Fatal("checks replaced dialog or executed SQL")
			}
			test.Tap(confirm)
			waitUI(t, w)
			if w.Window.Canvas().Overlays().Top() != overlay || len(client.requests) != 1 || client.requests[0].Text != strings.TrimSuffix(sql, ";") || client.requests[0].Scope != "main" || client.requests[0].Write != (sql == "DELETE FROM items;") {
				t.Fatal("execution lost scope, repeated confirmation or failed", client.requests)
			}
		})
	}
}

func TestSQLFileWriteRespectsReadOnlyProtection(t *testing.T) {
	w, p := parityWindow(t)
	p.ReadOnly = true
	var err error
	p, err = w.Profiles.Save(context.Background(), p)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "write.sql")
	os.WriteFile(path, []byte("DELETE FROM items;"), 0600)
	w.sidebar.loadSQLFile(p, "main", path)
	waitUI(t, w)
	var confirm *widget.Button
	walkUpdateDialog(w.Window.Canvas().Overlays().Top(), func(object fyne.CanvasObject) {
		if b, ok := object.(*widget.Button); ok && b.Text == "确认执行" {
			confirm = b
		}
	})
	if confirm == nil || !confirm.Disabled() {
		t.Fatal("read-only write enabled")
	}
}
