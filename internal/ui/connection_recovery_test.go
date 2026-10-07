package ui

import (
	"context"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"github.com/ealink1/super-link/internal/infra/secrets"
)

func TestCredentialRecoveryDoesNotLeaveAStaleWarning(t *testing.T) {
	for _, finish := range []string{"保存", "取消"} {
		t.Run(finish, func(t *testing.T) {
			w := interactionWindow(t, func(context.Context, domain.Profile) (adapter.Client, error) { return uiFixtureClient{}, nil })
			ctx := context.Background()
			p := w.profiles[0]
			p.Config.Password = "expired-session-fixture"
			p, err := w.Profiles.Save(ctx, p)
			if err != nil {
				t.Fatal(err)
			}
			// Keep the sidebar's old revision to verify that recovery uses the
			// fresh metadata returned alongside the credential hydration error.
			// Simulate restarting with a lost session credential. The recovery UI
			// must not leave a second warning underneath the connection editor.
			w.Profiles.Vault = secrets.NewLocal(w.Root)
			w.selected = p.ID
			w.editSelected()
			waitUI(t, w)
			overlays := w.Window.Canvas().Overlays()
			if len(overlays.List()) != 1 {
				t.Fatalf("recovery stacked %d overlays; warning will reappear after %s", len(overlays.List()), finish)
			}
			content := connectionPopupContent(t, overlays.Top())
			var password *widget.Entry
			var remember *widget.Check
			var hintFound bool
			walkConnectionForm(content, func(object fyne.CanvasObject) {
				switch item := object.(type) {
				case *widget.Entry:
					if item.Password && item.PlaceHolder == "密码" {
						password = item
					}
				case *widget.Check:
					if item.Text == "记住密码" {
						remember = item
					}
				case *widget.Label:
					hintFound = hintFound || strings.Contains(item.Text, secrets.ErrMissing.Error())
				}
			})
			if password == nil || remember == nil || !hintFound {
				t.Fatal("recovery fields or inline explanation missing")
			}
			password.SetText("remembered-recovery-fixture")
			test.TapAt(remember, fyne.NewPos(16, remember.Size().Height/2))
			button := findButton(content, finish)
			if button == nil {
				t.Fatal("recovery action missing")
			}
			test.Tap(button)
			waitUI(t, w)
			if len(overlays.List()) != 0 {
				t.Fatal("stale credential warning appeared after closing editor")
			}
			if finish == "取消" {
				return
			}
			w.Profiles.Vault = secrets.NewLocal(w.Root)
			loaded, err := w.Profiles.Get(ctx, p.ID)
			if err != nil || !loaded.Config.SavePassword || loaded.Config.Password != "remembered-recovery-fixture" {
				t.Fatal("recovery save did not retain credentials", err)
			}
			w.editSelected()
			waitUI(t, w)
			if len(overlays.List()) != 1 {
				t.Fatal("valid remembered credentials still produced a warning")
			}
			test.Tap(findButton(connectionPopupContent(t, overlays.Top()), "取消"))
			waitUI(t, w)
		})
	}
}

func connectionPopupContent(t *testing.T, object fyne.CanvasObject) fyne.CanvasObject {
	t.Helper()
	if overlay, ok := object.(fyne.Widget); ok {
		for _, child := range test.WidgetRenderer(overlay).Objects() {
			if popup, ok := child.(*widget.PopUp); ok {
				return popup.Content
			}
		}
	}
	t.Fatalf("expected connection editor in overlay, got %T", object)
	return nil
}

func walkConnectionForm(object fyne.CanvasObject, visit func(fyne.CanvasObject)) {
	visit(object)
	var children []fyne.CanvasObject
	switch item := object.(type) {
	case *fyne.Container:
		children = item.Objects
	case *container.Scroll:
		children = []fyne.CanvasObject{item.Content}
	case *container.AppTabs:
		for _, tab := range item.Items {
			children = append(children, tab.Content)
		}
	case *widget.Form:
		for _, field := range item.Items {
			children = append(children, field.Widget)
		}
	}
	for _, child := range children {
		walkConnectionForm(child, visit)
	}
}
