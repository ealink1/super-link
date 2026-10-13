package ui

import (
	"math"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestShellButtonTextCentersWithoutAccumulatingOffset(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(newShellTheme())
	button := shellButton("主机管理", "server", false, func() {})
	button.Resize(fyne.NewSize(140, 40))
	renderer := test.WidgetRenderer(button)
	var label *widget.RichText
	var icon *canvas.Image
	for _, object := range renderer.Objects() {
		switch object := object.(type) {
		case *widget.RichText:
			label = object
		case *canvas.Image:
			icon = object
		}
	}
	if label == nil || icon == nil {
		t.Fatal("button label or icon missing")
	}
	var previous fyne.Position
	for range 3 {
		renderer.Refresh()
		renderer.Layout(button.Size())
		labelCenter := label.Position().Y + label.Size().Height/2
		iconCenter := icon.Position().Y + icon.Size().Height/2
		if math.Abs(float64(labelCenter-iconCenter)) > 0.01 {
			t.Fatalf("label center %g differs from icon center %g", labelCenter, iconCenter)
		}
		if previous != (fyne.Position{}) && label.Position() != previous {
			t.Fatal("refresh accumulated a label offset")
		}
		previous = label.Position()
	}
}
