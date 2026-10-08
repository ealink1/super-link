package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"strings"
	"testing"
)

func TestGridTextPreviewTracksContentAndWidth(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	var preview gridTextPreview
	value := strings.Repeat("字段preview", 20)
	narrow := preview.fit(value, 80, 14, fyne.TextStyle{})
	if !strings.HasSuffix(narrow, "…") || fyne.MeasureText(narrow, 14, fyne.TextStyle{}).Width > 80 {
		t.Fatal("preview does not fit cell")
	}
	if preview.fit(value, 80, 14, fyne.TextStyle{}) != narrow {
		t.Fatal("cached preview changed")
	}
	if preview.fit("短值", 80, 14, fyne.TextStyle{}) != "短值" {
		t.Fatal("stale preview after rebinding")
	}
	if preview.fit(value, 5000, 14, fyne.TextStyle{}) != value {
		t.Fatal("stale preview after resizing")
	}
}

func BenchmarkGridTextPreview(b *testing.B) {
	app := test.NewApp()
	defer app.Quit()
	value := strings.Repeat("字段preview", 20)
	b.Run("uncached", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			fitText(value, 100, 14, fyne.TextStyle{})
		}
	})
	b.Run("cached", func(b *testing.B) {
		var preview gridTextPreview
		preview.fit(value, 100, 14, fyne.TextStyle{})
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			preview.fit(value, 100, 14, fyne.TextStyle{})
		}
	})
}
