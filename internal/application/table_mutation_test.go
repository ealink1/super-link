package application

import (
	"context"
	"errors"
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestTableMutationRequiresConfirmationBeforeDriver(t *testing.T) {
	for _, sql := range []string{"DROP TABLE items;", "TRUNCATE TABLE items;", "DELETE FROM items;"} {
		e, p, f := tableEngine(t, false)
		action := "structure"
		if sql == "DELETE FROM items;" {
			action = ""
		}
		request := domain.Execution{Revision: p.Revision, Scope: "main", Text: sql, Write: true, Action: action, MaxRows: 1}
		_, err := e.Execute(context.Background(), p.ID, request)
		var required *domain.ConfirmationRequired
		if !errors.As(err, &required) || f.read != 0 {
			t.Fatalf("%s reached driver without confirmation: %v", sql, err)
		}
		request.Confirmation = required.Fingerprint
		if _, err = e.Execute(context.Background(), p.ID, request); err != nil || f.read != 1 {
			t.Fatalf("confirmed %s: %v, calls %d", sql, err, f.read)
		}
	}
	if err := validateStructureScript(domain.Execution{Text: "TRUNCATE TABLE items; DELETE FROM items;", Action: "structure"}, "mysql"); err == nil {
		t.Fatal("structure action accepted DELETE")
	}
}
