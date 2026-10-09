package application

import (
	"context"
	"errors"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
)

type dropBatchClient struct {
	*fakeClient
	statements []string
	failAt     int
}

func (c *dropBatchClient) Execute(_ context.Context, request domain.Execution) ([]domain.Result, error) {
	c.statements = append(c.statements, request.Text)
	if len(c.statements) == c.failAt {
		return nil, errors.New("fixture write failure")
	}
	return []domain.Result{{}}, nil
}

func TestDropTablesBindsConfirmationToExactSelection(t *testing.T) {
	e, p, _ := testEngine(t, false)
	f := &dropBatchClient{fakeClient: &fakeClient{}}
	e.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return f, nil }
	objects := []domain.Object{{Kind: "table", Scope: "main", Name: "first"}, {Kind: "table", Scope: "main", Name: "second"}}
	_, err := e.DropTables(context.Background(), p.ID, p.Revision, objects, "", nil)
	var grant *domain.ConfirmationRequired
	if !errors.As(err, &grant) || len(f.statements) != 0 {
		t.Fatal("executed before confirmation", err)
	}
	token := grant.Fingerprint
	changed := append([]domain.Object(nil), objects...)
	changed[1].Name = "different"
	_, err = e.DropTables(context.Background(), p.ID, p.Revision, changed, token, nil)
	if !errors.As(err, &grant) || len(f.statements) != 0 {
		t.Fatal("changed targets executed", err)
	}
	_, err = e.DropTables(context.Background(), p.ID, p.Revision, objects, "", nil)
	if !errors.As(err, &grant) {
		t.Fatal(err)
	}
	progress := 0
	completed, err := e.DropTables(context.Background(), p.ID, p.Revision, objects, grant.Fingerprint, func(done, total int) {
		progress = done
		if total != 2 {
			t.Fatal(total)
		}
	})
	if err != nil || len(completed) != 2 || len(f.statements) != 2 || progress != 2 {
		t.Fatal(completed, err, f.statements)
	}
	if f.statements[0] != `DROP TABLE "first";` || f.statements[1] != `DROP TABLE "second";` {
		t.Fatal(f.statements)
	}
	_, err = e.DropTables(context.Background(), p.ID, p.Revision, objects, grant.Fingerprint, nil)
	if !errors.As(err, &grant) || len(f.statements) != 2 {
		t.Fatal("reused batch grant", err)
	}
}

func TestDropTablesStopsOnPartialFailure(t *testing.T) {
	e, p, _ := testEngine(t, false)
	f := &dropBatchClient{fakeClient: &fakeClient{}, failAt: 2}
	e.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return f, nil }
	objects := []domain.Object{{Kind: "table", Name: "first"}, {Kind: "table", Name: "second"}, {Kind: "table", Name: "third"}}
	_, err := e.DropTables(context.Background(), p.ID, p.Revision, objects, "", nil)
	var grant *domain.ConfirmationRequired
	if !errors.As(err, &grant) {
		t.Fatal(err)
	}
	completed, err := e.DropTables(context.Background(), p.ID, p.Revision, objects, grant.Fingerprint, nil)
	if err == nil || len(completed) != 1 || completed[0] != objects[0] || len(f.statements) != 2 {
		t.Fatal("partial failure lost or continued", completed, err, f.statements)
	}
}

func TestDropTablesRejectsProtectedAndInvalidTargets(t *testing.T) {
	e, p, f := testEngine(t, true)
	objects := []domain.Object{{Kind: "table", Name: "items"}}
	if _, err := e.DropTables(context.Background(), p.ID, p.Revision, objects, "", nil); !errors.Is(err, domain.ErrReadOnly) {
		t.Fatal(err)
	}
	p.ReadOnly = false
	p.Config.Protection.RestrictStructureEdit = true
	var err error
	p, err = e.Profiles.Save(context.Background(), p)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = e.DropTables(context.Background(), p.ID, p.Revision, objects, "", nil); !errors.Is(err, domain.ErrReadOnly) {
		t.Fatal(err)
	}
	if f.calls.Load() != 0 {
		t.Fatal("protected delete reached driver")
	}
}
