package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"testing"
)

func TestMessageDialogsHaveNoDecorativeStatusIcons(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	window := app.NewWindow("dialogs")
	defer window.Close()
	window.Resize(fyne.NewSize(800, 600))
	window.Show()
	information := newInformationDialog("完成", "处理已完成", window)
	information.Show()
	assertNoMessageStatusIcon(t, window)
	information.Hide()
	responses := []bool{}
	confirmation := newConfirmDialog("确认", "继续操作？", func(ok bool) { responses = append(responses, ok) }, window)
	confirmation.Show()
	assertNoMessageStatusIcon(t, window)
	confirmation.Confirm()
	if len(responses) != 1 || !responses[0] {
		t.Fatal("confirmation response changed", responses)
	}
	confirmation = newConfirmDialog("确认", "继续操作？", func(ok bool) { responses = append(responses, ok) }, window)
	confirmation.Show()
	confirmation.Hide()
	if len(responses) != 2 || responses[1] {
		t.Fatal("dismissal accepted operation", responses)
	}
}

func assertNoMessageStatusIcon(t *testing.T, window fyne.Window) {
	t.Helper()
	for _, overlay := range window.Canvas().Overlays().List() {
		walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
			if im, ok := object.(*canvas.Image); ok && im.Resource != nil {
				for _, resource := range []fyne.Resource{theme.InfoIcon(), theme.ErrorIcon(), theme.QuestionIcon()} {
					if im.Resource.Name() == resource.Name() {
						t.Error("decorative dialog icon remains", resource.Name())
					}
				}
			}
		})
	}
}
