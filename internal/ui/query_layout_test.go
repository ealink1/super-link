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

func TestLongSQLKeepsQueryAndResultViewportBounded(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "long-layout", Text: "SELECT 1;", Scope: "main"})
	waitUI(t, w)
	split := querySplitForTest(s)
	original := split.MinSize()
	for _, size := range []fyne.Size{fyne.NewSize(1100, 650), fyne.NewSize(850, 480)} {
		split.Resize(size)
		for _, text := range []string{strings.Repeat("SELECT 1 AS id; -- 中文行\n", 300), "SELECT '" + strings.Repeat("abcdefgh", 256) + "';"} {
			s.editor.SetText(text)
			for _, wrap := range []fyne.TextWrap{fyne.TextWrapOff, fyne.TextWrapWord, fyne.TextWrapOff} {
				s.editor.Wrapping = wrap
				s.editor.Refresh()
				if got := split.MinSize(); got != original {
					t.Fatalf("SQL text changed workspace minimum from %v to %v (wrap=%v)", original, got, wrap)
				}
				test.WidgetRenderer(split).Layout(size)
				if split.Trailing.Size().Height < 100 || split.Leading.Size().Height > size.Height || split.Trailing.Position().Y+split.Trailing.Size().Height > size.Height+.1 {
					t.Fatalf("editor pushed results out of viewport: query=%v results=%v at %v", split.Leading.Size(), split.Trailing.Size(), split.Trailing.Position())
				}
				if s.editor.Text != text {
					t.Fatal("layout or wrapping altered SQL source")
				}
			}
		}
	}
}

func TestLongSQLScrollsLineNumbersWithoutGrowingViewport(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "scroll-layout", Text: strings.Repeat("SELECT 1;\n", 300), Scope: "main"})
	waitUI(t, w)
	split := querySplitForTest(s)
	split.Resize(fyne.NewSize(1100, 650))
	gutter := queryGutterForTest(split.Leading)
	if gutter == nil {
		t.Fatal("line-number viewport missing")
	}
	var codeScroll *container.Scroll
	for _, object := range test.WidgetRenderer(s.code).Objects() {
		if scroll, ok := object.(*container.Scroll); ok {
			codeScroll = scroll
		}
	}
	if codeScroll == nil {
		t.Fatal("native editor scroll missing")
	}
	viewport := gutter.Size()
	// Drive the user-scroll notification without native scrollbar-hide timers,
	// which the software test driver's DoFromGoroutine runs outside this thread.
	codeScroll.ScrollToOffset(fyne.NewPos(0, 600))
	codeScroll.OnScrolled(codeScroll.Offset)
	if codeScroll.Offset.Y == 0 || gutter.Offset.Y != codeScroll.Offset.Y || gutter.Offset.X != 0 || gutter.Size() != viewport {
		t.Fatalf("line numbers lost scroll synchronization: code=%v gutter=%v", codeScroll.Offset, gutter.Offset)
	}
	codeScroll.ScrollToOffset(fyne.Position{})
	codeScroll.OnScrolled(codeScroll.Offset)
	if gutter.Offset.Y != 0 || gutter.Content.Position().Y != 0 {
		t.Fatal("line numbers did not return to document start")
	}
	w.Window.Canvas().Focus(s.code)
	for _, row := range []int{250, 300} {
		s.editor.CursorRow = row
		s.editor.CursorColumn = 3
		s.editor.Refresh()
		if codeScroll.Offset.Y == 0 || gutter.Offset.Y != codeScroll.Offset.Y {
			t.Fatalf("cursor navigation left line numbers at the previous scroll offset: code=%v gutter=%v row=%d", codeScroll.Offset, gutter.Offset, s.editor.CursorRow)
		}
	}
	s.editor.SetText("SELECT 2;")
	if split.MinSize().Height > 400 || gutter.Offset.Y != 0 {
		t.Fatal("shorter SQL retained document height or a stale line-number offset")
	}
}

func queryGutterForTest(object fyne.CanvasObject) *container.Scroll {
	if group, ok := object.(*fyne.Container); ok {
		if _, ok := group.Layout.(queryGutterLayout); ok {
			return group.Objects[0].(*container.Scroll)
		}
		for _, child := range group.Objects {
			if found := queryGutterForTest(child); found != nil {
				return found
			}
		}
	}
	return nil
}

func TestQueryDividerCanBeDraggedFromNearbyPixels(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "drag-layout", Text: strings.Repeat("SELECT 1;\n", 300), Scope: "main"})
	waitUI(t, w)
	w.Window.Resize(fyne.NewSize(1100, 740))
	split := querySplitForTest(s)
	for _, delta := range []float32{100, -100} {
		divider := test.WidgetRenderer(split).Objects()[1]
		position := w.App.Driver().AbsolutePositionForObject(divider).Add(fyne.NewPos(divider.Size().Width/2, 3))
		before := split.Leading.Size().Height
		test.Drag(w.Window.Canvas(), position, 0, delta)
		after := split.Leading.Size().Height
		if delta > 0 && after-before < 80 || delta < 0 && before-after < 80 {
			t.Fatalf("divider ignored vertical drag: before=%v after=%v delta=%v", before, after, delta)
		}
		if split.Trailing.Size().Height < 60 || split.Leading.Size().Height < 150 {
			t.Fatal("drag collapsed a pane")
		}
	}
}

func querySplitForTest(s *workspace) *querySplit {
	return s.item.Content.(*container.ThemeOverride).Content.(*querySplit)
}

func TestQueryDividerPaintsThinLineAcrossItsHitArea(t *testing.T) {
	a := test.NewApp()
	defer a.Quit()
	w := a.NewWindow("divider appearance")
	defer w.Close()
	w.SetPadded(false)
	top, bottom := canvas.NewRectangle(color.Transparent), canvas.NewRectangle(color.Transparent)
	split := newQuerySplit(top, bottom)
	w.SetContent(container.NewThemeOverride(split, querySplitTheme{}))
	w.Resize(fyne.NewSize(400, 300))
	w.Show()
	for _, dark := range []bool{false, true, false} {
		a.Settings().SetTheme(Theme{Dark: dark})
		background := color.NRGBAModel.Convert(a.Settings().Theme().Color(theme.ColorNameBackground, theme.VariantLight))
		top.FillColor, bottom.FillColor = background, background
		top.Refresh()
		bottom.Refresh()
		divider := test.WidgetRenderer(split).Objects()[1]
		hover := divider.(desktop.Hoverable)
		for _, hovered := range []bool{false, true, false} {
			if hovered {
				hover.MouseIn(&desktop.MouseEvent{})
			} else {
				hover.MouseOut()
			}
			capture := w.Canvas().Capture()
			position := a.Driver().AbsolutePositionForObject(divider)
			for _, x := range []int{1, 20, 200, 398} {
				inkRows := 0
				for y := int(position.Y); y < int(position.Y+divider.Size().Height); y++ {
					if color.NRGBAModel.Convert(capture.At(x, y)) != background {
						inkRows++
					}
				}
				if inkRows < 1 || inkRows > 2 {
					t.Fatalf("divider should span the pane as a thin line at x=%d, dark=%v hovered=%v: %d painted rows", x, dark, hovered, inkRows)
				}
			}
		}
	}
}
