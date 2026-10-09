package painter

import (
	"bytes"
	"image"
	"image/color"
	"os"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/shaping"
)

type italicTestMap struct{ face *font.Face }

func (m italicTestMap) ResolveFace(rune) *font.Face { return m.face }

func TestSyntheticItalicChineseFallbackAndBold(t *testing.T) {
	for _, name := range []string{"NaviUI-Regular.otf", "NaviUI-Bold.otf"} {
		data, err := os.ReadFile("../../../../internal/ui/assets/" + name)
		if err != nil {
			t.Fatal(err)
		}
		face, err := font.ParseTTF(bytes.NewReader(data))
		if err != nil {
			t.Fatal(err)
		}
		for _, ch := range "中文倾斜" {
			if _, ok := face.NominalGlyph(ch); !ok {
				t.Fatalf("missing CJK fixture glyph %c", ch)
			}
		}
		plain := image.NewRGBA(image.Rect(0, 0, 200, 80))
		italic := image.NewRGBA(plain.Bounds())
		fm := italicTestMap{face}
		bold := name == "NaviUI-Bold.otf"
		DrawString(plain, "中文倾斜", color.Black, fm, 32, 1, fyne.TextStyle{Bold: bold})
		DrawString(italic, "中文倾斜", color.Black, fm, 32, 1, fyne.TextStyle{Bold: bold, Italic: true})
		if bytes.Equal(plain.Pix, italic.Pix) {
			t.Fatal("Chinese fallback remained upright")
		}
		uprightSize, _ := MeasureString(fm, "中文倾斜", 32, fyne.TextStyle{Bold: bold})
		italicSize, _ := MeasureString(fm, "中文倾斜", 32, fyne.TextStyle{Bold: bold, Italic: true})
		if uprightSize != italicSize {
			t.Fatal("synthetic italic moved layout/caret advances")
		}
	}
}
func TestRealItalicFacesAreNotShearedAgain(t *testing.T) {
	face, err := font.ParseTTF(bytes.NewReader(theme.DefaultTextItalicFont().Content()))
	if err != nil {
		t.Fatal(err)
	}
	if needsSyntheticItalic(shaping.Output{Face: face}) {
		t.Fatal("real italic gets double slant")
	}
}
