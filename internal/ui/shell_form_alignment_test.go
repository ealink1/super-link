package ui

import (
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestShellFormInputRemainsVisibleAfterRefresh(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	e := newShellHostEditor(w.switcher.shell, domain.ShellHost{Port: 22, User: "root", Remember: true})
	e.modal.popup.Show()
	defer e.modal.hide()
	for _, dark := range []bool{false, true} {
		w.setDark(dark)
		for _, entry := range []*shellFormEntry{e.name, e.password, e.port} {
			test.Type(entry, "中Ab9")
			entry.Refresh()
			found := false
			for _, object := range test.WidgetRenderer(entry).Objects() {
				if scroll, ok := object.(*container.Scroll); ok {
					found = true
					if scroll.Size().Width <= 0 || scroll.Size().Height < scroll.Content.MinSize().Height || scroll.Position().Y < 0 || scroll.Position().Y+scroll.Size().Height > entry.Size().Height {
						t.Fatalf("input content clipped after refresh: viewport %v at %v, content %v, input %v", scroll.Size(), scroll.Position(), scroll.Content.MinSize(), entry.Size())
					}
				}
			}
			if !found {
				t.Fatal("single-line input lost its scroll viewport")
			}
		}
	}
}
