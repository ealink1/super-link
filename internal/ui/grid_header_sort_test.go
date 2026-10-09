package ui

import (
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestGridHeaderSortTargetsBoundColumnAndHidesUtilityHeaders(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	var column string
	var descending bool
	current := true
	model := gridModel{columns: []domain.Column{{Name: "id"}, {Name: "name"}}, length: func() int { return 0 }, current: func() bool { return current }, sortColumn: func(name string, desc bool) { column, descending = name, desc }}
	h := newGridHeader(model, widget.NewTable(nil, nil, nil))
	h.bind(2)
	if !h.sortControls.Visible() {
		t.Fatal("data column has no sort controls")
	}
	test.Tap(h.ascending)
	if column != "id" || descending {
		t.Fatal("ascending targeted wrong field")
	}
	h.bind(3)
	test.Tap(h.descending)
	if column != "name" || !descending {
		t.Fatal("rebound descending targeted stale field")
	}
	current = false
	test.Tap(h.ascending)
	if !descending {
		t.Fatal("retired grid changed sorting")
	}
	for _, index := range []int{0, 1} {
		h.bind(index)
		if h.sortControls.Visible() {
			t.Fatal("utility header exposes sort controls", index)
		}
	}
}
