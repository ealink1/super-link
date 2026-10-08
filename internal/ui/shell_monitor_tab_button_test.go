package ui

import (
	"image"
	"image/color"
	"math"
	"reflect"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestMonitorTabIconAndLabelPaintedAtSameHeight(t *testing.T) {
	w, _ := fileActionFixture(t)
	for _, name := range []string{"综合", "CPU", "GPU", "MEM", "DISK", "NET"} {
		tapped := false
		shade := color.NRGBA{124, 126, 128, 255}
		button := newMonitorTabButton(name, monitorMetricIcon(name, shade), func() { tapped = true })
		object := container.NewThemeOverride(button, monitorNavigationTheme{monitorTintTheme: monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 10}, shade: shade}})
		w.Window.SetContent(object)
		w.Window.Resize(fyne.NewSize(80, 28))
		renderer := test.WidgetRenderer(button)
		var icon *canvas.Image
		var label *widget.Label
		for _, object := range renderer.Objects() {
			switch o := object.(type) {
			case *canvas.Image:
				icon = o
			case *widget.Label:
				label = o
			}
		}
		if icon == nil || label == nil {
			t.Fatal("missing tab content", name)
		}
		im := w.Window.Canvas().Capture()
		iconInk, ok := monitorTabInkBounds(im, image.Rect(int(icon.Position().X), 0, int(icon.Position().X+icon.Size().Width+1), 28))
		if !ok {
			t.Fatal("icon not painted", name)
		}
		textInk, ok := monitorTabInkBounds(im, image.Rect(int(label.Position().X), 0, 80, 28))
		if !ok {
			t.Fatal("label not painted", name)
		}
		if math.Abs(float64(iconInk.Min.Y+iconInk.Max.Y-textInk.Min.Y-textInk.Max.Y)/2) > 1.5 {
			t.Fatalf("%s misaligned: icon %v, label %v", name, iconInk, textInk)
		}
		test.Tap(button)
		if !tapped {
			t.Fatal("tab cannot be activated", name)
		}
	}
}

// Small regular CJK glyphs are mostly antialiased pixels; include their visible
// coverage instead of requiring a pixel to exactly match the foreground color.
func monitorTabInkBounds(im image.Image, area image.Rectangle) (image.Rectangle, bool) {
	bounds := image.Rectangle{Min: area.Max, Max: area.Min}
	found := false
	for y := area.Min.Y; y < area.Max.Y; y++ {
		for x := area.Min.X; x < area.Max.X; x++ {
			c := color.NRGBAModel.Convert(im.At(x, y)).(color.NRGBA)
			if c.R < 220 && c.G < 220 && c.B < 220 && math.Abs(float64(c.R)-float64(c.G)) < 12 && math.Abs(float64(c.G)-float64(c.B)) < 12 {
				found = true
				bounds.Min.X = min(bounds.Min.X, x)
				bounds.Min.Y = min(bounds.Min.Y, y)
				bounds.Max.X = max(bounds.Max.X, x+1)
				bounds.Max.Y = max(bounds.Max.Y, y+1)
			}
		}
	}
	return bounds, found
}

func TestMonitorTabHoverDoesNotMoveContent(t *testing.T) {
	w, _ := fileActionFixture(t)
	for _, name := range []string{"综合", "CPU", "GPU", "MEM", "DISK", "NET"} {
		shade := color.NRGBA{124, 126, 128, 255}
		button := newMonitorTabButton(name, monitorMetricIcon(name, shade), func() {})
		w.Window.SetContent(container.NewThemeOverride(button, monitorNavigationTheme{monitorTintTheme: monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 10}, shade: shade}}))
		w.Window.Resize(fyne.NewSize(80, 28))
		geometry := func() []fyne.Position {
			w.Window.Canvas().Capture()
			var result []fyne.Position
			for _, object := range test.WidgetRenderer(button).Objects() {
				switch object.(type) {
				case *canvas.Image, *widget.Label:
					result = append(result, object.Position(), fyne.NewPos(object.Size().Width, object.Size().Height))
				}
			}
			return result
		}
		initial := geometry()
		for i := 0; i < 3; i++ {
			button.MouseIn(&desktop.MouseEvent{})
			hovered := geometry()
			button.MouseOut()
			normal := geometry()
			button.FocusGained()
			focused := geometry()
			button.FocusLost()
			unfocused := geometry()
			if !reflect.DeepEqual(initial, focused) || !reflect.DeepEqual(initial, unfocused) {
				t.Fatalf("%s focus moved content", name)
			}
			if !reflect.DeepEqual(initial, hovered) || !reflect.DeepEqual(initial, normal) {
				t.Fatalf("%s hover moved content: initial %v, hover %v, out %v", name, initial, hovered, normal)
			}
		}
	}
}
