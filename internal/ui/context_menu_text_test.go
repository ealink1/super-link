package ui

import (
	"bytes"
	"image"
	"image/color"
	"math"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/go-text/render"
	"github.com/go-text/typesetting/di"
	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/shaping"
	"golang.org/x/image/math/fixed"
)

// Rasterize the Chinese face into an oversized image. A second Fyne text
// capture could reproduce the same clipped texture and hide the regression.
func TestContextMenuChineseInkFitsTextTexture(t *testing.T) {
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	window := app.NewWindow("menu clipping")
	defer window.Close()
	window.Resize(fyne.NewSize(320, 240))
	window.Show()
	face, err := font.ParseTTF(bytes.NewReader(cjkRegularFont.Content()))
	if err != nil {
		t.Fatal(err)
	}
	shaper := shaping.HarfbuzzShaper{}
	labels := []string{"关闭左边标签", "关闭右边标签", "关闭其他标签", "关闭所有标签", "复制 INSERT 模板"}
	items := make([]*fyne.MenuItem, 0, len(labels))
	for i, label := range labels {
		item := fyne.NewMenuItem(label, func() {})
		item.Disabled = i == 1
		items = append(items, item)
	}
	showContextMenu(fyne.NewMenu("", items...), window.Canvas(), fyne.NewPos(20, 20))
	popup := window.Canvas().Focused().(*widget.PopUpMenu)
	defer popup.Hide()
	for i, item := range popup.Items {
		var text *canvas.Text
		for _, object := range test.WidgetRenderer(item.(fyne.Widget)).Objects() {
			if value, ok := object.(*canvas.Text); ok {
				text = value
				break
			}
		}
		if text == nil {
			t.Fatal("menu text missing")
		}
		value := []rune(labels[i])
		run := shaper.Shape(shaping.Input{Text: value, RunEnd: len(value), Direction: di.DirectionLTR, Face: face, Size: fixed.Int26_6(text.TextSize * 64)})
		for _, scale := range []float32{1, 1.25, 1.5, 2} {
			height := int(math.Round(float64(text.MinSize().Height * scale)))
			im := image.NewRGBA(image.Rect(0, 0, 250, height+30))
			raster := render.Renderer{FontSize: text.TextSize, PixScale: scale, Color: color.Black}
			baseline := int(math.Ceil(float64(run.LineBounds.Ascent) * float64(scale) / 64))
			raster.DrawShapedRunAt(run, im, 0, baseline)
			for y := height; y < im.Bounds().Dy(); y++ {
				for x := 0; x < im.Bounds().Dx(); x++ {
					if im.RGBAAt(x, y).A != 0 {
						t.Errorf("%s at scale %g loses bottom ink at y=%d in a %d-pixel text texture", text.Text, scale, y, height)
						goto nextScale
					}
				}
			}
		nextScale:
		}
		if text.Position().Y+text.Size().Height > item.Size().Height {
			t.Error("text extends below its menu row")
		}
		if item.Position().Y+item.Size().Height > popup.Size().Height {
			t.Error("last menu row extends below the popup")
		}
	}
}
