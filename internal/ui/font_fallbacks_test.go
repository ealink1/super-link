package ui

import (
	"bytes"
	"testing"

	"fyne.io/fyne/v2"
	"github.com/go-text/typesetting/font"
)

func TestCompactChineseFallbackPreservesDesktopFontAndStyle(t *testing.T) {
	for _, style := range []fyne.TextStyle{{}, {Bold: true}, {Italic: true}, {Monospace: true}, {Monospace: true, Bold: true}} {
		if (Theme{}).Font(style) != desktopFonts.font(style) {
			t.Fatal("fallback replaced the desktop font")
		}
		resources := (Theme{}).FontFallbacks(style)
		if len(resources) != 1 {
			t.Fatal("missing compact fallback")
		}
		face, err := font.ParseTTF(bytes.NewReader(resources[0].Content()))
		if err != nil {
			t.Fatal(err)
		}
		for _, r := range "中文数据库终端网络接口内存磁盘运行时间" {
			if _, ok := face.NominalGlyph(r); !ok {
				t.Fatalf("fallback lacks %c", r)
			}
		}
		bold := face.Font.Describe().Aspect.Weight >= font.WeightSemibold
		if bold != style.Bold {
			t.Fatalf("wrong CJK weight for %+v", style)
		}
	}
}
