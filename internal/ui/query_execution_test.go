package ui

import (
	"context"
	"errors"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
)

type queryExecutionClient struct {
	parityFixture
	requests chan domain.Execution
	err      error
}

func (c *queryExecutionClient) Execute(_ context.Context, request domain.Execution) ([]domain.Result, error) {
	c.requests <- request
	if c.err != nil {
		return nil, c.err
	}
	return []domain.Result{{RowsAffected: 1}}, nil
}

func installQueryExecutionClient(t *testing.T, w *Window, p domain.Profile, err error) *queryExecutionClient {
	t.Helper()
	if err := w.Engine.Disconnect(t.Context(), p.ID); err != nil {
		t.Fatal(err)
	}
	client := &queryExecutionClient{requests: make(chan domain.Execution, 8), err: err}
	w.Engine.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return client, nil }
	return client
}

func TestQueryWriteExecutesWithoutConfirmationDialog(t *testing.T) {
	for _, sql := range []string{"ALTER TABLE items ADD COLUMN remark TEXT", "UPDATE items SET name = :name WHERE id = 1", "SELECT 1"} {
		t.Run(sql, func(t *testing.T) {
			w, p := parityWindow(t)
			client := installQueryExecutionClient(t, w, p, nil)
			s := w.openWorkspace(p, domain.Draft{ID: "direct-sql", Text: sql, Scope: "main"})
			if field := s.parameters["name"]; field != nil {
				field.value.SetText("updated")
				s.code.TypedShortcut(&desktop.CustomShortcut{KeyName: fyne.KeyR, Modifier: fyne.KeyModifierSuper})
			} else {
				test.Tap(s.execute)
			}
			waitUI(t, w)
			if w.Window.Canvas().Overlays().Top() != nil || len(client.requests) != 1 || s.busy || !strings.HasPrefix(s.status.Text, "执行完成") {
				t.Fatalf("SQL did not execute once without a dialog: %s, calls=%d", s.status.Text, len(client.requests))
			}
			request := <-client.requests
			if request.Scope != "main" || request.Revision != p.Revision || request.Write != (sql != "SELECT 1") {
				t.Fatal("execution lost scope, revision or write classification")
			}
			if strings.HasPrefix(sql, "UPDATE") && (len(request.Args) != 1 || request.Args[0] != "updated") {
				t.Fatal("direct execution lost bound parameters")
			}
			if s.lastRequest.Text != sql || s.lastRequest.Confirmation != "" {
				t.Fatal("result provenance changed or retained an internal confirmation token")
			}
		})
	}
}

func TestQueryWriteFailureIsNotRetried(t *testing.T) {
	for _, cause := range []error{errors.New("fixture write interrupted"), &domain.ConfirmationRequired{Fingerprint: "fixture-driver-error"}} {
		t.Run(cause.Error(), func(t *testing.T) {
			w, p := parityWindow(t)
			client := installQueryExecutionClient(t, w, p, cause)
			s := w.openWorkspace(p, domain.Draft{ID: "failed-sql", Text: "DELETE FROM items", Scope: "main"})
			test.Tap(s.execute)
			waitUI(t, w)
			if len(client.requests) != 1 || s.busy || !strings.Contains(s.status.Text, cause.Error()) || !strings.Contains(s.status.Text, "outcome may be unknown") {
				t.Fatalf("failed write was retried or lost its error: %s, calls=%d", s.status.Text, len(client.requests))
			}
			if w.Window.Canvas().Overlays().Top() != nil {
				t.Fatal("write failure opened a confirmation dialog")
			}
		})
	}
}

func TestQueryDirectWritePreservesExecutionProtection(t *testing.T) {
	for _, policy := range []string{"read-only", "script-protection", "revision-conflict"} {
		t.Run(policy, func(t *testing.T) {
			w, p := parityWindow(t)
			client := installQueryExecutionClient(t, w, p, nil)
			s := w.openWorkspace(p, domain.Draft{ID: "protected-sql", Text: "DELETE FROM items", Scope: "main"})
			if policy == "read-only" {
				p.ReadOnly = true
			} else if policy == "script-protection" {
				p.Config.Protection.RestrictScriptExecution = true
			}
			saved, err := w.Profiles.Save(t.Context(), p)
			if err != nil {
				t.Fatal(err)
			}
			want := domain.ErrConflict
			if policy != "revision-conflict" {
				s.profile = saved
				want = domain.ErrReadOnly
			}
			s.runAdaptive()
			waitUI(t, w)
			if len(client.requests) != 0 || !strings.Contains(s.status.Text, want.Error()) || s.busy {
				t.Fatalf("execution policy bypassed: %s, calls=%d", s.status.Text, len(client.requests))
			}
			if w.Window.Canvas().Overlays().Top() != nil {
				t.Fatal("blocked SQL opened a confirmation dialog")
			}
		})
	}
}
