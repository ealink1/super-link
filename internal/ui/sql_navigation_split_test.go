package ui

import (
	"image/color"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/domain"
)

func TestSQLNavigationDividerDragsFromNearbyPixels(t *testing.T) {
	w, p := parityWindow(t)
	text := strings.Repeat("SELECT 1;\n", 300)
	s := w.openWorkspace(p, domain.Draft{ID: "sidebar-drag", Text: text, Scope: "main"})
	w.Window.Resize(fyne.NewSize(1400, 800))
	waitUI(t, w)
	object, split := sqlNavigationSplitForTest(w.switcher.sql)
	if split == nil {
		t.Fatal("SQL navigation split missing")
	}
	for _, delta := range []float32{100, -100} {
		divider := queryDividerForTest(object)
		position := w.App.Driver().AbsolutePositionForObject(divider).Add(fyne.NewPos(3, divider.Size().Height/2))
		before := split.Leading.Size().Width
		test.Drag(w.Window.Canvas(), position, delta, 0)
		after := split.Leading.Size().Width
		if delta > 0 && after-before < 80 || delta < 0 && before-after < 80 {
			t.Fatalf("sidebar ignored horizontal drag: before=%v after=%v delta=%v", before, after, delta)
		}
		if split.Leading.Size().Width < split.Leading.MinSize().Width-.1 || split.Trailing.Size().Width < split.Trailing.MinSize().Width-.1 {
			t.Fatal("drag collapsed navigation or document pane")
		}
		if s.editor.Text != text || querySplitForTest(s).Trailing.Size().Height < 60 {
			t.Fatal("sidebar drag altered SQL or hid the result pane")
		}
	}
}

func TestSQLNavigationDividerPaintsThinVerticalLine(t *testing.T) {
	a := test.NewApp()
	defer a.Quit()
	w := a.NewWindow("sidebar divider appearance")
	defer w.Close()
	w.SetPadded(false)
	left, right := canvas.NewRectangle(color.Transparent), canvas.NewRectangle(color.Transparent)
	split := newQueryNavigationSplit(left, right)
	w.SetContent(split)
	w.Resize(fyne.NewSize(400, 300))
	w.Show()
	for _, dark := range []bool{false, true, false} {
		a.Settings().SetTheme(Theme{Dark: dark})
		background := color.NRGBAModel.Convert(a.Settings().Theme().Color(theme.ColorNameBackground, theme.VariantLight))
		left.FillColor, right.FillColor = background, background
		left.Refresh()
		right.Refresh()
		divider := queryDividerForTest(split)
		hover := divider.(desktop.Hoverable)
		for _, hovered := range []bool{false, true, false} {
			if hovered {
				hover.MouseIn(&desktop.MouseEvent{})
			} else {
				hover.MouseOut()
			}
			capture := w.Canvas().Capture()
			position := a.Driver().AbsolutePositionForObject(divider)
			for _, y := range []int{1, 20, 150, 298} {
				inkColumns := 0
				for x := int(position.X); x < int(position.X+divider.Size().Width); x++ {
					if color.NRGBAModel.Convert(capture.At(x, y)) != background {
						inkColumns++
					}
				}
				if inkColumns < 1 || inkColumns > 2 {
					t.Fatalf("sidebar divider should remain a thin full-height line at y=%d, dark=%v hovered=%v: %d painted columns", y, dark, hovered, inkColumns)
				}
			}
		}
	}
}

func sqlNavigationSplitForTest(object fyne.CanvasObject) (fyne.Widget, *container.Split) {
	switch object := object.(type) {
	case *container.Split:
		if object.Horizontal {
			return object, object
		}
	case *querySplit:
		if object.Horizontal {
			return object, &object.Split
		}
	case *container.ThemeOverride:
		return sqlNavigationSplitForTest(object.Content)
	case *fyne.Container:
		for _, child := range object.Objects {
			if widget, split := sqlNavigationSplitForTest(child); split != nil {
				return widget, split
			}
		}
	}
	return nil, nil
}
