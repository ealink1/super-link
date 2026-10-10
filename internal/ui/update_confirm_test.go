package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"testing"
)

func TestUpdateConfirmShowsReleaseAndRequiresAccept(t *testing.T) {
	w := shellTestWindow(t)
	accepted := 0
	for _, chooseAccept := range []bool{false, true} {
		w.showUpdateConfirm("0.1.11", "0.1.14", 48_024_780, func() { accepted++ })
		var cancel, confirm *widget.Button
		seen := map[string]bool{}
		for _, overlay := range w.Window.Canvas().Overlays().List() {
			walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
				if text, ok := object.(*canvas.Text); ok {
					seen[text.Text] = true
				}
				button, ok := object.(*widget.Button)
				if !ok || button.Icon == nil {
					return
				}

				switch button.Icon.Name() {
				case theme.CancelIcon().Name():
					cancel = button
				case theme.DownloadIcon().Name():
					confirm = button
				}
			})
		}
		for _, value := range []string{"发现新版本", "0.1.11", "0.1.14", "NEW", "安装包大小"} {
			if !seen[value] {
				t.Fatalf("missing release detail %q", value)
			}
		}
		if cancel == nil || confirm == nil {
			t.Fatal("missing symbol buttons")
		}
		if chooseAccept {
			test.Tap(confirm)
		} else {
			test.Tap(cancel)
		}
		if !chooseAccept && accepted != 0 {
			t.Fatal("cancel accepted the update")
		}
	}
	if accepted != 1 {
		t.Fatal("accept did not run exactly once", accepted)
	}
}
