package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"strings"
	"testing"
)

func TestNavigatorIconHintsAppearAndPreserveFilterActions(t *testing.T) {
	w, _ := parityWindow(t)
	buttons := map[string]*tableActionButton{}
	walkUpdateDialog(w.Window.Content(), func(object fyne.CanvasObject) {
		if button, ok := object.(*tableActionButton); ok {
			for name, hint := range navigatorActionHints {
				if button.hint == hint {
					buttons[name] = button
				}
			}
		}
	})
	if len(buttons) != len(navigatorActionHints) {
		t.Fatal("sidebar actions lack hints", len(buttons))
	}
	for name, button := range buttons {
		button.MouseIn(nil)
		if !w.docTooltip.box.Visible() || w.docTooltip.label.Text != navigatorActionHints[name] {
			t.Fatal("missing hover description", name)
		}
		if w.Window.Canvas().Overlays().Top() != nil {
			t.Fatal("hint captures clicks")
		}
		button.MouseOut()
		if w.docTooltip.box.Visible() {
			t.Fatal("hint survived mouse exit", name)
		}
	}
	button := buttons["view"]
	button.MouseIn(nil)
	test.Tap(button)
	if w.sidebar.kindFilter != "view" || w.docTooltip.box.Visible() {
		t.Fatal("view filter or hint dismissal changed")
	}
	for name, hint := range navigatorActionHints {
		if !strings.Contains(hint, "\n") {
			t.Fatal("missing description", name)
		}
	}
}

func TestNavigatorTooltipFollowsPointerWithinSidebar(t *testing.T) {
	w, _ := parityWindow(t)
	var button *tableActionButton
	walkUpdateDialog(w.Window.Content(), func(object fyne.CanvasObject) {
		if candidate, ok := object.(*tableActionButton); ok && candidate.hint == navigatorActionHints["view"] {
			button = candidate
		}
	})
	if button == nil {
		t.Fatal("view button missing")
	}
	origin := w.App.Driver().AbsolutePositionForObject(w.docTooltip.layer)
	point := origin.Add(fyne.NewPos(60, 100))
	button.MouseIn(&desktop.MouseEvent{PointEvent: fyne.PointEvent{AbsolutePosition: point}})
	if w.docTooltip.box.Position() != fyne.NewPos(60, 116) {
		t.Fatal("tooltip did not appear below pointer in sidebar", w.docTooltip.box.Position())
	}
	point = point.Add(fyne.NewPos(5, 3))
	button.MouseMoved(&desktop.MouseEvent{PointEvent: fyne.PointEvent{AbsolutePosition: point}})
	if w.docTooltip.box.Position() != fyne.NewPos(65, 119) {
		t.Fatal("tooltip did not follow pointer", w.docTooltip.box.Position())
	}
	point = origin.Add(fyne.NewPos(w.docTooltip.layer.Size().Width-2, w.docTooltip.layer.Size().Height-2))
	button.MouseMoved(&desktop.MouseEvent{PointEvent: fyne.PointEvent{AbsolutePosition: point}})
	box := w.docTooltip.box
	if box.Position().X+box.Size().Width > w.docTooltip.layer.Size().Width || box.Position().Y+box.Size().Height > w.docTooltip.layer.Size().Height {
		t.Fatal("tooltip exceeds window edge")
	}
	button.MouseOut()
}
