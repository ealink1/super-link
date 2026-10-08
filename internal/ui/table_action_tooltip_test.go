package ui

import (
	"github.com/ealink1/super-link/internal/domain"
	"strings"
	"testing"
)

func TestTableActionHoverExplainsDisabledSaveWithoutCapturingClicks(t *testing.T) {
	w, p := parityWindow(t)
	page := w.openTable(p, domain.Object{Name: "items", Scope: "main", Schema: "public", Kind: "table"})
	waitUI(t, w)
	button := page.commitButton
	button.MouseIn(nil)
	if !w.docTooltip.box.Visible() || !strings.Contains(w.docTooltip.label.Text, "保存修改") {
		t.Fatal("save explanation missing")
	}
	if w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("tooltip intercepts actions")
	}
	button.MouseOut()
	if w.docTooltip.box.Visible() {
		t.Fatal("tooltip remained after mouse exit")
	}
	for name, hint := range tableActionHints {
		if !strings.Contains(hint, "\n") {
			t.Fatal("missing detailed action hint", name)
		}
	}
}
