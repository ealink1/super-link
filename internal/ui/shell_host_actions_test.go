package ui

import (
	"context"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/secrets"
)

func TestHostCardMenuFloatsAndCopiesRememberedPassword(t *testing.T) {
	w := shellTestWindow(t)
	w.Profiles.Vault = secrets.NewLocal(w.Root)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	host, err := s.service.Save(context.Background(), domain.ShellHost{Name: "dd", Host: "127.0.0.1", Port: 22, User: "root", Password: "s3cr3t", Remember: true})
	if err != nil {
		t.Fatal(err)
	}
	card := newShellHostCard(s)
	w.Window.SetContent(card)
	card.bind(host, false)
	card.Resize(fyne.NewSize(300, 240))
	waitUI(t, w)

	test.Tap(card.menu)
	waitUI(t, w)
	popup, rows := hostMenuPopup(t, w)
	if len(rows) != 3 || rows[0].Text != "编辑" || rows[1].Text != "复制密码" || rows[2].Text != "删除" {
		var labels []string
		for _, row := range rows {
			labels = append(labels, row.Text)
		}
		t.Fatalf("menu rows: %v", labels)
	}
	if !strings.Contains(string(rows[2].Icon.Content()), "#ff5877") {
		t.Fatal("删除 was not tinted destructive")
	}
	if strings.Contains(string(rows[0].Icon.Content()), "#ff5877") {
		t.Fatal("编辑 inherited the destructive tint")
	}
	origin := w.App.Driver().AbsolutePositionForObject(card.menu)
	if position := popup.Position(); position.Y < origin.Y || position.X > origin.X+card.menu.Size().Width {
		t.Fatalf("menu did not open beside the trigger: menu=%v button=%v", position, origin)
	}

	test.Tap(rows[1])
	waitUI(t, w)
	if got := w.App.Clipboard().Content(); got != "s3cr3t" {
		t.Fatalf("clipboard after 复制密码: %q", got)
	}
	if open := len(w.Window.Canvas().Overlays().List()); open != 0 {
		t.Fatalf("menu stayed open after the action: %d overlays", open)
	}
}

func TestHostCardMenuReportsHostWithoutRememberedPassword(t *testing.T) {
	w := shellTestWindow(t)
	w.Profiles.Vault = secrets.NewLocal(w.Root)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	host, err := s.service.Save(context.Background(), domain.ShellHost{Name: "plain", Host: "127.0.0.1", Port: 22, User: "root"})
	if err != nil {
		t.Fatal(err)
	}
	s.copyHostPassword(host)
	waitUI(t, w)
	if got := w.App.Clipboard().Content(); got != "" {
		t.Fatalf("an unremembered secret reached the clipboard: %q", got)
	}
	if open := len(w.Window.Canvas().Overlays().List()); open == 0 {
		t.Fatal("the missing password went unreported")
	}
}

func hostMenuPopup(t *testing.T, w *Window) (*widget.PopUp, []*shellAlignedButton) {
	t.Helper()
	var popup *widget.PopUp
	var rows []*shellAlignedButton
	var walk func(fyne.CanvasObject)
	walk = func(object fyne.CanvasObject) {
		switch v := object.(type) {
		case *widget.PopUp:
			popup = v
		case *shellAlignedButton:
			rows = append(rows, v)
		}
		if native, ok := object.(fyne.Widget); ok {
			for _, child := range test.WidgetRenderer(native).Objects() {
				walk(child)
			}
		}
		if branch, ok := object.(*fyne.Container); ok {
			for _, child := range branch.Objects {
				walk(child)
			}
		}
	}
	for _, object := range w.Window.Canvas().Overlays().List() {
		walk(object)
	}
	if popup == nil {
		t.Fatal("host actions did not open a floating menu")
	}
	return popup, rows
}
