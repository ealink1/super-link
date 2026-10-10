//go:build !race

// In a full-package -race run the first Capture() below reflects a renderer cache
// left by an earlier test, so the same Han label measures 14px in one sample and
// 18px in another. The guard stays active for the normal and CI native runs.
package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image"
	"image/color"
	"math"
	"testing"
)

func TestResultTabChineseAndLatinInkAlignment(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	window := app.NewWindow("alignment")
	defer window.Close()
	tabs := newResultTabs(container.NewTabItem("数据", widget.NewLabel("")), container.NewTabItem("字段", widget.NewLabel("")), container.NewTabItem("DDL", widget.NewLabel("")))
	window.SetContent(tabs)
	window.Show()
	tabs.Refresh()
	shade := color.NRGBAModel.Convert(app.Settings().Theme().Color(theme.ColorNameForeground, theme.VariantLight)).(color.NRGBA)
	var centers []float64
	for _, item := range tabs.Items {
		cached := tabs.buttons[item]
		cached.state.selected = false
		window.SetContent(cached.content)
		window.Resize(fyne.NewSize(80, 34))
		cached.button.Refresh()
		im := window.Canvas().Capture()
		bounds, ok := shellFileInkBounds(im, image.Rect(5, 0, 75, 34), shade)
		if !ok {
			t.Fatal("tab text missing", item.Text)
		}
		centers = append(centers, float64(bounds.Min.Y+bounds.Max.Y)/2)
	}
	for _, center := range centers[1:] {
		if math.Abs(center-centers[0]) > 1.5 {
			t.Fatal("Chinese/Latin tab ink centers differ", centers)
		}
	}
}
