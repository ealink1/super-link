package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"strings"
	"testing"
)

func TestCatalogDeleteButtonPreviewsSelectionBeforeExecuting(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openDatabaseTables(p, "main")
	waitUI(t, w)
	if page.deleteButton == nil || page.deleteButton.Icon == nil || !page.deleteButton.Disabled() {
		t.Fatal("missing disabled delete icon")
	}
	first := page.shown[0]
	second := domain.Object{Kind: "table", Scope: "main", Schema: "public", Name: "other"}
	page.objects = append(page.objects, second)
	page.filter()
	page.selectTableRow(second, 0)
	page.selectTableRow(first, fyne.KeyModifierControl)
	if page.deleteButton.Disabled() {
		t.Fatal("selection did not enable delete")
	}
	targets := page.deletionTargets()
	if len(targets) != 2 || targets[0] != first || targets[1] != second {
		t.Fatal("targets not in display order", targets)
	}
	page.deleteButton.OnTapped()
	waitUI(t, w)
	if !page.deleting || !page.deleteButton.Disabled() || len(w.Window.Canvas().Overlays().List()) == 0 {
		t.Fatal("delete did not show confirmation or prevent duplicate submission")
	}
	var cancelButton *widget.Button
	previewed := false
	walkUpdateDialog(w.Window.Canvas().Overlays().Top(), func(object fyne.CanvasObject) {
		if button, ok := object.(*widget.Button); ok && button.Text == "取消" {
			cancelButton = button
		}
		if label, ok := object.(*widget.Label); ok && strings.Contains(label.Text, first.Name) && strings.Contains(label.Text, second.Name) {
			previewed = true
		}
	})
	if cancelButton == nil || !previewed {
		t.Fatal("confirmation is missing selection names or cancel button")
	}
	test.Tap(cancelButton)
	if page.deleting || len(page.selectedTables) != 2 || page.deleteButton.Disabled() {
		t.Fatal("cancel changed selection or left deletion busy")
	}
	page.profile.ReadOnly = true
	page.updateDeleteButton()
	if !page.deleteButton.Disabled() {
		t.Fatal("read-only delete enabled")
	}
	w.closeDatabaseTables(page)
	if !page.closed {
		t.Fatal("page did not close")
	}
}
