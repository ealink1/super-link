package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"testing"
)

func TestUpdateConfirmUsesOnlySymbolsAndRequiresAccept(t *testing.T) {
	w := shellTestWindow(t)
	accepted := 0
	for _, chooseAccept := range []bool{false, true} {
		w.showUpdateConfirm("版本更新", func() { accepted++ })
		var cancel, confirm *widget.Button
		for _, overlay := range w.Window.Canvas().Overlays().List() {
			walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
				button, ok := object.(*widget.Button)
				if !ok || button.Icon == nil {
					return
				}
				if button.Text != "" {
					t.Fatal("confirmation button contains text", button.Text)
				}
				switch button.Icon.Name() {
				case theme.CancelIcon().Name():
					cancel = button
				case theme.ConfirmIcon().Name():
					confirm = button
				}
			})
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
