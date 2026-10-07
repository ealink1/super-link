package ui

import (
	"context"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"github.com/ealink1/super-link/internal/infra/secrets"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestRememberPasswordDefaultsAndRetainsExistingChoice(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	for _, tc := range []struct {
		name         string
		id           string
		stored, want bool
	}{
		{name: "new connection", want: true},
		{name: "existing session only", id: "existing", want: false},
		{name: "existing remembered", id: "existing", stored: true, want: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			e := &connectionEditor{original: domain.Profile{ID: tc.id, Config: connection.ConnectionConfig{Type: "mysql", SavePassword: tc.stored}}}
			e.initFields()
			if e.persist.Text != "记住密码" || e.persist.Checked != tc.want {
				t.Fatal("remember password choice changed")
			}
		})
	}
}

func TestRememberPasswordEditorSaveRestoresWithoutKeychain(t *testing.T) {
	w := interactionWindow(t, func(context.Context, domain.Profile) (adapter.Client, error) { return uiFixtureClient{}, nil })
	w.Profiles.Vault = secrets.NewLocal(w.Root)
	p := w.profiles[0]
	e := &connectionEditor{owner: w, original: p}
	e.show()
	e.password.SetText("editor-fixture")
	test.TapAt(e.persist, fyne.NewPos(16, e.persist.Size().Height/2))
	e.connectAfterSave.SetChecked(false)
	// Exercise the actual Save button, collection, background persistence and
	// completion callback, then restore credentials through a fresh vault.
	test.Tap(findButton(e.modal.Content, "保存"))
	waitUI(t, w)
	if !e.closed || e.saving {
		t.Fatal("connection form did not save", e.hint.Text)
	}
	w.Profiles.Vault = secrets.NewLocal(w.Root)
	loaded, err := w.Profiles.Get(context.Background(), p.ID)
	if err != nil || !loaded.Config.SavePassword || loaded.Config.Password != "editor-fixture" {
		t.Fatal("remembered editor password did not restore", err)
	}
}
