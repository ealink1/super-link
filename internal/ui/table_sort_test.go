package ui

import (
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"strings"
	"testing"
)

func TestQuickSortPreservesFiltersResetsPageAndBuildsServerOrder(t *testing.T) {
	w, p := parityWindow(t)
	table := w.openTable(p, domain.Object{Name: "items", Scope: "main", Kind: "table"})
	waitUI(t, w)
	table.request.Condition = "id > 0"
	for _, descending := range []bool{false, true} {
		table.request.Page = 2
		table.applyQuickSort("id", descending)
		waitUI(t, w)
		if table.request.Page != 1 || table.request.Condition != "id > 0" || len(table.request.Sorts) != 1 || table.request.Sorts[0].Descending != descending {
			t.Fatal("quick sort lost query state", table.request)
		}
		query, _, err := sqlworkbench.BuildPage(p.SQLDialect(), table.request, table.page.Info)
		direction := "ASC"
		if descending {
			direction = "DESC"
		}
		if err != nil || !strings.Contains(query, `ORDER BY "id" `+direction) {
			t.Fatal(query, err)
		}
		if len(table.sorts) != 1 || table.sorts[0].column.Selected != "id" || table.sorts[0].direction.Selected != direction {
			t.Fatal("sort panel is out of sync")
		}
	}
	table.applyQuickSort("missing", true)
	if table.request.Sorts[0].Column != "id" || table.busy {
		t.Fatal("invalid field changed ordering")
	}
	table.inserts = []domain.RowChange{{}}
	table.applyQuickSort("name", false)
	if table.request.Sorts[0].Column != "id" || table.busy {
		t.Fatal("sorting discarded pending edits")
	}
}
