package ui

import (
	"fmt"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
)

func groupSelectForm(t *testing.T) (*Window, *connectionGroupForm) {
	t.Helper()
	w, _ := parityWindow(t)
	if err := w.Profiles.AddGroup(t.Context(), "生产环境"); err != nil {
		t.Fatal(err)
	}
	if err := w.Profiles.CreateGroupWithOptions(t.Context(), "测试环境", "", domain.ConnectionGroupOptions{}); err != nil {
		t.Fatal(err)
	}
	if err := w.Profiles.AddGroup(t.Context(), "归档"); err != nil {
		t.Fatal(err)
	}
	w.Window.Resize(fyne.NewSize(1000, 800))
	manager := newConnectionGroups(w)
	manager.show()
	waitUI(t, w)
	f := newConnectionGroupForm(manager)
	f.show()
	waitUI(t, w)
	return w, f
}

func TestConnectionGroupSelectMouseKeyboardAndDismiss(t *testing.T) {
	w, f := groupSelectForm(t)
	s := f.parent
	test.Tap(s)
	if s.popup == nil || !s.popup.Visible() {
		t.Fatal("dropdown did not open")
	}
	if s.popup.Size().Width != s.Size().Width {
		t.Fatal("popover width does not match field", s.popup.Size(), s.Size())
	}
	origin := w.App.Driver().AbsolutePositionForObject(s)
	if s.popup.Position().Y > origin.Y || s.popup.Position().Y+s.popup.Size().Height < origin.Y {
		t.Fatal("native-style menu does not align over current option")
	}
	captureConnectionIndicators(t, w.Window, "parent-group-dropdown.png")
	// Click the actual overlay canvas so the list's recycled row receives the tap.
	test.TapCanvas(w.Window.Canvas(), s.popup.Position().Add(fyne.NewPos(80, 40)))
	if s.Selected != "生产环境" || s.popup.Visible() {
		t.Fatal("mouse selection failed", s.Selected)
	}
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeySpace})
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeyDown})
	if s.Selected != "生产环境" {
		t.Fatal("arrow navigation committed before confirmation")
	}
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
	if s.Selected != "测试环境" || s.popup.Visible() {
		t.Fatal("keyboard selection failed", s.Selected)
	}
	test.Tap(s)
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeyEscape})
	if s.popup.Visible() || s.Selected != "测试环境" {
		t.Fatal("escape did not cancel")
	}
	test.Tap(s)
	test.TapCanvas(w.Window.Canvas(), fyne.NewPos(2, 2))
	if s.popup.Visible() || s.Selected != "测试环境" {
		t.Fatal("outside click did not cancel")
	}
	test.Tap(s)
	f.close()
	if s.popup.Visible() {
		t.Fatal("dropdown outlived form")
	}
}

func TestConnectionGroupSelectBoundsAndDisabledState(t *testing.T) {
	w, f := groupSelectForm(t)
	s := f.parent
	for index := 0; index < 30; index++ {
		s.Options = append(s.Options, fmt.Sprintf("机房 %d", index))
	}
	test.Tap(s)
	if s.popup.Size().Height > 200 {
		t.Fatal("too many rows overflow the popover")
	}
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeyDown})
	f.setSaving(true)
	if s.popup.Visible() {
		t.Fatal("disabled dropdown left open")
	}
	before := s.Selected
	test.Tap(s)
	s.TypedKey(&fyne.KeyEvent{Name: fyne.KeySpace})
	if s.popup.Visible() || s.Selected != before {
		t.Fatal("disabled selection changed")
	}
	f.setSaving(false)
	s.closePopup()
	w.Window.Resize(fyne.NewSize(700, 520))
	f.popup.Resize(fyne.NewSize(460, 472))
	test.Tap(s)
	size, pos := s.popup.Size(), s.popup.Position()
	screen := w.Window.Canvas().Size()
	if pos.X < 0 || pos.Y < 0 || pos.X+size.Width > screen.Width || pos.Y+size.Height > screen.Height {
		t.Fatal("popover not bounded by canvas", pos, size, screen)
	}
	captureConnectionIndicators(t, w.Window, "parent-group-dropdown-compact.png")
}
