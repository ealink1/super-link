package ui

import (
	"image"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
)

func TestHostCardHighlightDoesNotFillVirtualRow(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	s.visible = []domain.ShellHost{{ID: "one", Name: "Test host", Host: "127.0.0.1", User: "tester", Port: 22}}
	h := newShellHostCollection(s)
	w.Window.SetContent(h)
	w.Window.Resize(fyne.NewSize(960, 400))
	h.Refresh()
	before := w.Window.Canvas().Capture()
	test.MoveMouse(w.Window.Canvas(), fyne.NewPos(100, 100))
	hovered := w.Window.Canvas().Capture()
	assertHostRegionEqual(t, before, hovered, image.Rect(380, 20, 900, 230))
	if hostRegionEqual(before, hovered, image.Rect(20, 20, 290, 230)) {
		t.Fatal("hover did not highlight the card")
	}
	h.list.Select(0)
	w.Window.Canvas().Focus(h.list)
	selected := w.Window.Canvas().Capture()
	assertHostRegionEqual(t, before, selected, image.Rect(380, 20, 900, 230))
	test.MoveMouse(w.Window.Canvas(), fyne.NewPos(800, 100))
	emptyHover := w.Window.Canvas().Capture()
	assertHostRegionEqual(t, before, emptyHover, image.Rect(380, 20, 900, 230))
	assertHostRegionEqual(t, before, emptyHover, image.Rect(20, 20, 290, 230))
}

func TestHostCardRebindClearsPreviousHover(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	card := newShellHostCard(w.switcher.shell)
	card.bind(domain.ShellHost{ID: "one", Name: "First"}, false)
	card.MouseIn(&desktop.MouseEvent{})
	if !card.hovered {
		t.Fatal("card did not receive hover")
	}
	card.bind(domain.ShellHost{ID: "two", Name: "Second"}, false)
	if card.hovered {
		t.Fatal("recycled card retained another host's highlight")
	}
	card.bind(domain.ShellHost{ID: "two", Name: "Second"}, true)
	if card.background == nil {
		t.Fatal("list mode lost card background")
	}
}

func assertHostRegionEqual(t *testing.T, a, b image.Image, area image.Rectangle) {
	t.Helper()
	if !hostRegionEqual(a, b, area) {
		t.Fatalf("highlight changed unrelated region %v", area)
	}
}
func hostRegionEqual(a, b image.Image, area image.Rectangle) bool {
	for y := area.Min.Y; y < area.Max.Y; y++ {
		for x := area.Min.X; x < area.Max.X; x++ {
			ar, ag, ab, aa := a.At(x, y).RGBA()
			br, bg, bb, ba := b.At(x, y).RGBA()
			if ar != br || ag != bg || ab != bb || aa != ba {
				return false
			}
		}
	}
	return true
}
