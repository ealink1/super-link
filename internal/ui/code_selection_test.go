package ui

import (
	"fmt"
	"image"
	"image/color"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
)

func TestCodeSelectionKeepsSQLVisible(t *testing.T) {
	for _, dark := range []bool{false, true} {
		for _, wrap := range []fyne.TextWrap{fyne.TextWrapOff, fyne.TextWrapWord} {
			t.Run(fmt.Sprintf("dark=%v/wrap=%v", dark, wrap), func(t *testing.T) {
				a := test.NewApp()
				defer a.Quit()
				a.Settings().SetTheme(Theme{Dark: dark})
				w := a.NewWindow("SQL selection")
				defer w.Close()
				e := newCodeEntry()
				e.Wrapping = wrap
				w.SetContent(container.NewThemeOverride(e, codeTheme{state: e.colors}))
				w.Resize(fyne.NewSize(700, 180))
				w.Show()
				const sql = "ALTER TABLE items\n  ADD COLUMN remark VARCHAR(64) DEFAULT '中文备注';\n-- 保留选中的 SQL"
				e.SetText(sql)
				e.Refresh()
				w.Canvas().Focus(e)
				before := w.Canvas().Capture()
				palette := []color.Color{tokenColor("keyword", dark), tokenColor("string", dark), tokenColor("comment", dark)}
				if wrap != fyne.TextWrapOff {
					palette = []color.Color{(Theme{Dark: dark}).Color(theme.ColorNameForeground, theme.VariantLight)}
				}
				originalCounts := make([]int, len(palette))
				for i, ink := range palette {
					originalCounts[i] = countCodeInk(before, ink, 80)
				}
				e.TypedShortcut(&fyne.ShortcutSelectAll{})
				selected := w.Canvas().Capture()
				if e.SelectedText() != sql {
					t.Fatal("selection did not cover the complete SQL")
				}
				for i, ink := range palette {
					originalInk := originalCounts[i]
					selectedInk := countCodeInk(selected, ink, 80)
					if originalInk < 10 || selectedInk < originalInk/2 {
						t.Fatalf("selected SQL lost visible text: before=%d selected=%d color=%v", originalInk, selectedInk, ink)
					}
				}
				selection := (Theme{Dark: dark}).Color(theme.ColorNameSelection, theme.VariantLight)
				if countCodeInk(selected, selection, 5) < 100 {
					t.Fatal("selection background disappeared")
				}
				clipboard := w.Clipboard()
				e.TypedShortcut(&fyne.ShortcutCopy{Clipboard: clipboard})
				if clipboard.Content() != sql {
					t.Fatal("copy changed selected SQL")
				}
				e.TypedRune('x')
				if e.Text != "x" {
					t.Fatal("selection replacement stopped working")
				}
			})
		}
	}
}

func countCodeInk(capture image.Image, ink color.Color, tolerance int) int {
	want := color.NRGBAModel.Convert(ink).(color.NRGBA)
	count := 0
	for y := capture.Bounds().Min.Y; y < capture.Bounds().Max.Y; y++ {
		for x := capture.Bounds().Min.X; x < capture.Bounds().Max.X; x++ {
			got := color.NRGBAModel.Convert(capture.At(x, y)).(color.NRGBA)
			difference := 0
			for _, delta := range [3]int{int(got.R) - int(want.R), int(got.G) - int(want.G), int(got.B) - int(want.B)} {
				difference += max(delta, -delta)
			}
			if difference < tolerance {
				count++
			}
		}
	}
	return count
}
