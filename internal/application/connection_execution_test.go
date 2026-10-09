package application

import (
	"context"
	"database/sql"
	"errors"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
)

type closedExecutionClient struct {
	*fakeClient
	err error
}

func (c *closedExecutionClient) Execute(context.Context, domain.Execution) ([]domain.Result, error) {
	c.calls.Add(1)
	return nil, c.err
}

func TestClosedSessionRecoveryNeverReplaysUncertainWrite(t *testing.T) {
	for _, tc := range []struct {
		name    string
		failure error
		recover bool
	}{
		{"already closed", sql.ErrConnDone, true},
		{"transport unknown", errors.New("connection reset after sending"), false},
		{"server error", errors.New("table not found"), false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			e, _, _ := testEngine(t, false)
			p := saveProfile(t, e.Profiles, "mysql", false)
			old := &closedExecutionClient{fakeClient: &fakeClient{}, err: tc.failure}
			fresh := &fakeClient{}
			opens := 0
			e.Factory = func(context.Context, domain.Profile) (adapter.Client, error) {
				opens++
				if opens == 1 {
					return old, nil
				}
				return fresh, nil
			}
			req := domain.Execution{Revision: p.Revision, Scope: "main", Text: "DROP TABLE items;", Action: "structure", Write: true}
			_, err := e.Execute(context.Background(), p.ID, req)
			var required *domain.ConfirmationRequired
			if !errors.As(err, &required) || opens != 0 {
				t.Fatal("connected before confirmation", err)
			}
			req.Confirmation = required.Fingerprint
			_, err = e.Execute(context.Background(), p.ID, req)
			if tc.recover {
				if err != nil || opens != 2 || old.calls.Load() != 1 || fresh.calls.Load() != 1 || e.ConnectionStatuses()[p.ID] != ConnectionConnected {
					t.Fatalf("recovery failed: %v opens=%d", err, opens)
				}
			} else if err == nil || opens != 1 || fresh.calls.Load() != 0 {
				t.Fatal("uncertain write replayed", err)
			}
		})
	}
}

func TestClosedSessionReconnectFailureDoesNotPanic(t *testing.T) {
	e, _, _ := testEngine(t, false)
	p := saveProfile(t, e.Profiles, "mysql", false)
	opens := 0
	e.Factory = func(context.Context, domain.Profile) (adapter.Client, error) {
		opens++
		if opens == 1 {
			return &closedExecutionClient{fakeClient: &fakeClient{}, err: sql.ErrConnDone}, nil
		}
		return nil, errors.New("reconnect failed")
	}
	req := domain.Execution{Revision: p.Revision, Text: "DROP TABLE items;", Action: "structure", Write: true}
	_, err := e.Execute(context.Background(), p.ID, req)
	var required *domain.ConfirmationRequired
	if !errors.As(err, &required) {
		t.Fatal(err)
	}
	req.Confirmation = required.Fingerprint
	if _, err = e.Execute(context.Background(), p.ID, req); err == nil {
		t.Fatal("lost failure")
	}
}
