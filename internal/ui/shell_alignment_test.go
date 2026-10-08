package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"testing"
)

func TestShellButtonTextCentersWithoutAccumulatingOffset(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(newShellTheme())
	button := shellButton("主机管理", "server", false, func() {})
	button.Resize(fyne.NewSize(140, 40))
	renderer := test.WidgetRenderer(button)
	for range 3 {
		renderer.Refresh()
		renderer.Layout(button.Size())
		for _, object := range renderer.Objects() {
			if _, ok := object.(*widget.RichText); ok {
				want := (button.Size().Height-object.MinSize().Height)/2 - 2
				if object.Position().Y != want || object.Size().Height != object.MinSize().Height {
					t.Fatalf("text geometry: %v %v, want y %v", object.Position(), object.Size(), want)
				}
			}
		}
	}
}
