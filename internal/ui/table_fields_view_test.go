package ui

import (
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestFieldActionsReuseDesignerAndStageDeletion(t *testing.T) {
	w, p := parityWindow(t)
	table := w.openTable(p, domain.Object{Name: "items", Kind: "table"})
	waitUI(t, w)
	designer := table.fieldDesigner(0)
	if designer == nil || !designer.selected[0] {
		t.Fatal("field action did not select field")
	}
	if table.fieldDesigner(0) != designer || len(w.designers) != 1 {
		t.Fatal("field action created duplicate designer")
	}
	designer.deleteFields()
	if !designer.fields[0].deleted || !designer.dirty() {
		t.Fatal("field deletion was not staged")
	}
	if table.page.Info.Columns[0].Name != designer.fields[0].original {
		t.Fatal("field action changed loaded metadata")
	}
}
