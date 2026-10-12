package ui

import (
	"bytes"
	"image"
	"image/color"
	"math"
	"testing"

	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"github.com/go-text/render"
	"github.com/go-text/typesetting/di"
	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/shaping"
	"golang.org/x/image/math/fixed"
)

// Use an oversized font raster as the reference, rather than a second Fyne
// text texture that can suffer the same clipping as the object under test.
func TestMonitorPercentageFitsRoundedOpenGLTexture(t *testing.T) {
	shellTestWindow(t)
	face, err := font.ParseTTF(bytes.NewReader(monitorMonoFont.Content()))
	if err != nil {
		t.Fatal(err)
	}
	shaper := shaping.HarfbuzzShaper{}
	for _, value := range []string{"76%", "80%", "100%", "4.3%"} {
		label := newMonitorMonoLabel(value, 11, 0, monitorIPColor)
		label.Resize(label.MinSize())
		// Give the entire value its usual metric-card width.
		size := label.Size()
		size.Width = 100
		label.Resize(size)
		text := test.WidgetRenderer(label).Objects()[0].(*canvas.Text)
		run := shaper.Shape(shaping.Input{Text: []rune(text.Text), RunEnd: len([]rune(text.Text)), Direction: di.DirectionLTR, Face: face, Size: fixed.I(11)})
		for _, scale := range []float32{1, 1.25, 1.5, 2} {
			width := int(math.Round(float64(text.MinSize().Width * scale)))
			im := image.NewRGBA(image.Rect(0, 0, width+20, 60))
			renderer := render.Renderer{FontSize: text.TextSize, PixScale: scale, Color: color.Black}
			baseline := int(math.Ceil(float64(run.LineBounds.Ascent) * float64(scale) / 64))
			renderer.DrawShapedRunAt(run, im, 0, baseline)
			for y := 0; y < im.Bounds().Dy(); y++ {
				for x := width; x < im.Bounds().Dx(); x++ {
					if im.RGBAAt(x, y).A != 0 {
						t.Fatalf("%s at scale %g loses glyph ink at x=%d in a %d-pixel OpenGL texture", value, scale, x, width)
					}
				}
			}
		}
	}
}
