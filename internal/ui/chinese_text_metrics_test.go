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
	"github.com/go-text/render"
	"github.com/go-text/typesetting/di"
	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/shaping"
	"golang.org/x/image/math/fixed"
)

type chineseMetricFaces struct{ primary, fallback *font.Face }

func (f chineseMetricFaces) ResolveFace(r rune) *font.Face {
	if _, ok := f.primary.NominalGlyph(r); ok {
		return f.primary
	}
	return f.fallback
}

// Use independent Harfbuzz runs in a generously sized raster, rather than a
// Fyne capture that would silently apply the same incorrect texture clipping.
func TestChineseTextInkFitsMeasuredLine(t *testing.T) {
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	for _, style := range []fyne.TextStyle{{}, {Bold: true}, {Italic: true}, {Bold: true, Italic: true}, {Monospace: true}} {
		primary, err := font.ParseTTF(bytes.NewReader(Theme{}.Font(style).Content()))
		if err != nil {
			t.Fatal(err)
		}
		fallback, err := font.ParseTTF(bytes.NewReader(Theme{}.FontFallbacks(style)[0].Content()))
		if err != nil {
			t.Fatal(err)
		}
		faces := chineseMetricFaces{primary, fallback}
		for _, size := range []float32{10, 11, 12, 13, 16, 18, 22, 26, 32} {
			for _, value := range []string{"默认分组", "服务器地址与数据库连接", "FTP模式：被动模式", "中文Agyp混排：编辑"} {
				text := canvas.NewText(value, color.Black)
				text.TextSize, text.TextStyle = size, style
				runes := []rune(value)
				segmenter := shaping.Segmenter{}
				inputs := segmenter.Split(shaping.Input{Text: runes, RunEnd: len(runes), Direction: di.DirectionLTR, Size: fixed.Int26_6(size * 64)}, faces)
				shaper := shaping.HarfbuzzShaper{}
				var runs []shaping.Output
				var ascent fixed.Int26_6
				for _, input := range inputs {
					run := shaper.Shape(input)
					runs = append(runs, run)
					ascent = max(ascent, run.LineBounds.Ascent)
				}
				_, baseline := app.Driver().RenderedTextSize(value, size, style, nil)
				if math.Abs(float64(baseline)-float64(ascent)/64) > 0.02 {
					t.Errorf("%q size %g style %+v: measured baseline %g differs from drawn baseline %g", value, size, style, baseline, float32(ascent)/64)
				}
				for _, scale := range []float32{1, 1.25, 1.5, 2, 3} {
					height := int(math.Ceil(math.Round(float64(text.MinSize().Height*scale)) + 1))
					im := image.NewRGBA(image.Rect(0, 0, 1000, height+80))
					raster := render.Renderer{FontSize: size, PixScale: scale, Color: color.Black}
					x, y := float32(0), int(math.Ceil(float64(ascent)*float64(scale)/64))
					for _, run := range runs {
						raster.DrawShapedRunAt(run, im, int(x), y)
						x += float32(run.Advance) * scale / 64
					}
					for y := height; y < im.Bounds().Dy(); y++ {
						for x := 0; x < im.Bounds().Dx(); x++ {
							if im.RGBAAt(x, y).A != 0 {
								t.Errorf("%q size %g scale %g style %+v: bottom ink y=%d exceeds texture height %d", value, size, scale, style, y, height)
								goto nextScale
							}
						}
					}
				nextScale:
				}
			}
		}
	}
}
