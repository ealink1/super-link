package painter_test

import (
	"os"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/internal/painter"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"github.com/go-text/typesetting/di"
	"github.com/go-text/typesetting/shaping"
	"golang.org/x/image/math/fixed"
)

func TestFallbackLineMetricsCoverUsedFacesAndMatchDrawingBaseline(t *testing.T) {
	data, err := os.ReadFile("../../../../internal/ui/assets/NaviUI-Regular.otf")
	if err != nil {
		t.Fatal(err)
	}
	app := test.NewTempApp(t)
	app.Settings().SetTheme(fallbackTestTheme{Theme: theme.DefaultTheme(), fallback: fyne.NewStaticResource("cjk-line.otf", data)})
	painter.ClearFontCache()
	defer painter.ClearFontCache()
	faces := painter.CachedFontFace(fyne.TextStyle{}, nil, nil).Fonts
	for _, size := range []float32{11, 16, 32} {
		latinSize, latinBase := painter.MeasureString(faces, "Agyp", size, fyne.TextStyle{})
		for _, value := range []string{"中文", "Ag中文yp", "中文Agyp"} {
			runes := []rune(value)
			segmenter := shaping.Segmenter{}
			inputs := segmenter.Split(shaping.Input{Text: runes, RunEnd: len(runes), Direction: di.DirectionLTR, Size: fixed.Int26_6(size * 64)}, faces)
			shaper := shaping.HarfbuzzShaper{}
			var ascent fixed.Int26_6
			var outputs []shaping.Output
			for _, input := range inputs {
				output := shaper.Shape(input)
				ascent = max(ascent, output.LineBounds.Ascent)
				outputs = append(outputs, output)
			}
			measured, baseline := painter.MeasureString(faces, value, size, fyne.TextStyle{})
			if baseline != float32(ascent)/64 {
				t.Errorf("%q size %g: measured baseline %g differs from shaped baseline %g", value, size, baseline, float32(ascent)/64)
			}
			for _, output := range outputs {
				bottom := float32(ascent-output.LineBounds.Descent+output.LineBounds.Gap) / 64
				if measured.Height < bottom {
					t.Errorf("%q size %g: line height %g clips fallback line extent %g", value, size, measured.Height, bottom)
				}
			}
		}
		// Resolving a tall fallback must not enlarge later Latin-only strings.
		afterSize, afterBase := painter.MeasureString(faces, "Agyp", size, fyne.TextStyle{})
		if afterSize != latinSize || afterBase != latinBase {
			t.Fatal("fallback resolution changed Latin-only geometry")
		}
		for _, value := range []string{"", "\t", "\r"} {
			measured, baseline := painter.MeasureString(faces, value, size, fyne.TextStyle{})
			if measured.Height != latinSize.Height || baseline != latinBase {
				t.Errorf("empty/control text %q lost primary line metrics", value)
			}
		}
	}
}
