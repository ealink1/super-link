package ui

import (
	"image"
	"image/color"
	"math"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
)

func TestMonitorNetworkTextPaintedAtRowCenter(t *testing.T) {
	w, _ := fileActionFixture(t)
	for _, height := range []int{38, 60} {
		row := newMonitorDetailRow("enp1s0", "↓ 8KB/s", "↑ 19KB/s", 0, true)
		w.Window.SetContent(container.NewThemeOverride(row, newShellTheme()))
		w.Window.Resize(fyne.NewSize(360, float32(height)))
		im := w.Window.Canvas().Capture()
		for _, column := range []struct {
			name        string
			left, right int
			shade       color.NRGBA
		}{
			{"name", 12, 200, color.NRGBA{124, 126, 128, 255}},
			{"download", 235, 295, color.NRGBA{48, 163, 28, 255}},
			{"upload", 295, 350, color.NRGBA{43, 111, 255, 255}},
		} {
			bounds, ok := shellFileInkBounds(im, image.Rect(column.left, 0, column.right, height), column.shade)
			if !ok {
				t.Fatal("text missing", column.name)
			}
			center := float64(bounds.Min.Y+bounds.Max.Y) / 2
			if math.Abs(center-float64(height)/2) > 1.5 {
				t.Fatalf("%s painted off-center at height %d: %v", column.name, height, bounds)
			}
		}
	}
}
func TestMonitorSectionIconAndTextPaintedTogether(t *testing.T) {
	w, _ := fileActionFixture(t)
	section := monitorSection("网络接口", "网络", monitorAccent("网络"))
	w.Window.SetContent(container.NewThemeOverride(section, newShellTheme()))
	w.Window.Resize(fyne.NewSize(360, 20))
	im := w.Window.Canvas().Capture()
	icon, ok := shellFileInkBounds(im, image.Rect(0, 0, 12, 20), color.NRGBA{48, 163, 28, 255})
	if !ok {
		t.Fatal("section icon missing")
	}
	text, ok := shellFileInkBounds(im, image.Rect(18, 0, 100, 20), color.NRGBA{124, 126, 128, 255})
	if !ok {
		t.Fatal("section text missing")
	}
	if math.Abs(float64(icon.Min.Y+icon.Max.Y-text.Min.Y-text.Max.Y)/2) > 1.5 {
		t.Fatalf("section is misaligned: icon %v, text %v", icon, text)
	}
}
