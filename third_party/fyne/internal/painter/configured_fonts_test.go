package painter_test

import (
	"os"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/internal/painter"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

type fallbackTestTheme struct {
	fyne.Theme
	fallback fyne.Resource
}

func (t fallbackTestTheme) FontFallbacks(fyne.TextStyle) []fyne.Resource {
	return []fyne.Resource{nil, t.fallback}
}

func TestConfiguredCJKFallbackSurvivesLocalThemesAndExplicitFontSources(t *testing.T) {
	data, err := os.ReadFile("../../../../internal/ui/assets/NaviUI-Regular.otf")
	if err != nil {
		t.Fatal(err)
	}
	application := test.NewTempApp(t)
	application.Settings().SetTheme(fallbackTestTheme{Theme: theme.DefaultTheme(), fallback: fyne.NewStaticResource("cjk.otf", data)})
	painter.ClearFontCache()
	defer painter.ClearFontCache()
	label := widget.NewLabel("中文")
	_ = container.NewThemeOverride(label, theme.DefaultTheme())
	for _, source := range []fyne.Resource{nil, theme.DefaultTextMonospaceFont()} {
		faces := painter.CachedFontFace(fyne.TextStyle{}, source, label).Fonts
		latin := faces.ResolveFace('A')
		chinese := faces.ResolveFace('中')
		if chinese == latin {
			t.Fatal("primary Latin face unexpectedly covers Chinese")
		}
		if chinese.Font.Describe().Family != "NaviUI Regular" {
			t.Fatalf("configured CJK fallback was lost: %s", chinese.Font.Describe().Family)
		}
		for _, r := range "中文数据库服务器" {
			if got := faces.ResolveFace(r); got != chinese {
				t.Fatal("same CJK coverage reloaded a different font")
			}
			if _, ok := chinese.NominalGlyph(r); !ok {
				t.Fatalf("fallback lacks %c", r)
			}
		}
	}
}
