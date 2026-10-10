package ui

import (
	"os"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestAboutCardShowsIdentityAndPrivacyFacts(t *testing.T) {
	w := shellTestWindow(t)
	w.about()
	seen := map[string]bool{}
	var copy *widget.Button
	var dismiss *shellAlignedButton
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
			switch item := object.(type) {
			case *canvas.Text:
				seen[item.Text] = true
			case *widget.Label:
				seen[item.Text] = true
			case *widget.Button:
				if item.Icon != nil && strings.HasPrefix(item.Icon.Name(), "shell-copy") {
					copy = item
				}
			case *shellAlignedButton:
				if item.Text == "好" {
					dismiss = item
				}
			}
		})
	}
	for _, value := range []string{"SuperLink", "版本 test", "Go · Fyne", "数据库权限 600", "记住的密码加密保存", aboutDisplayPath(w.Root)} {
		if !seen[value] {
			t.Fatalf("missing about detail %q", value)
		}
	}
	if copy == nil || dismiss == nil {
		t.Fatal("missing copy or dismiss action")
	}
	test.Tap(copy)
	if content := fyne.CurrentApp().Clipboard().Content(); content != w.Root {
		t.Fatalf("clipboard holds %q, want the data directory %q", content, w.Root)
	}
	test.Tap(dismiss)
	if open := w.Window.Canvas().Overlays().List(); len(open) != 0 {
		t.Fatalf("dismissing the card left %d overlays open", len(open))
	}
}

func TestAboutDisplayPathAbbreviatesHome(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		t.Skip("the user home directory is unavailable")
	}
	if got := aboutDisplayPath(home + "/Library/Application Support/SuperLink"); got != "~/Library/Application Support/SuperLink" {
		t.Fatalf("got %q", got)
	}
	if got := aboutDisplayPath("/tmp/other/state.sqlite"); got != "/tmp/other/state.sqlite" {
		t.Fatalf("got %q", got)
	}
}
