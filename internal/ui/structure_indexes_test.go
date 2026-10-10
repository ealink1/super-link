package ui

import (
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestDesignerPendingIndexCanBeRemovedAndDropCanBeUndone(t *testing.T) {
	w, p := parityWindow(t)
	info, _ := (parityFixture{}).TableInfo(t.Context(), "main", "items")
	info.Indexes = []connection.IndexDefinition{{Name: "existing", ColumnName: "id"}, {Name: "existing", ColumnName: "name"}}
	w.designTable(p, domain.Object{Name: "items", Kind: "table"}, info)
	d := w.designers[w.tabs.Selected()]
	captureParity(t, w, "parity-structure.png")
	index := domain.NewIndex{Name: "idx_name", Method: "BTREE", Columns: []domain.IndexColumn{{Name: "name"}}}
	if !d.stageNewIndex(index) || d.stageNewIndex(index) {
		t.Fatal("index validation did not reject duplicates")
	}
	d.removeIndexes(map[int]bool{1: true}, false)
	if len(d.indexChanges) != 0 {
		t.Fatal("deleting new index left a CREATE or nonexistent DROP")
	}
	d.removeIndexes(map[int]bool{0: true}, false)
	if len(d.indexChanges) != 1 || d.indexChanges[0].OriginalName != "existing" {
		t.Fatal("composite index created duplicate drops")
	}
	d.removeIndexes(map[int]bool{0: true}, true)
	if len(d.indexChanges) != 0 || d.dirty() {
		t.Fatal("undo left a pending index change")
	}
}

func TestDesignerCompositeIndexDisplayPreservesColumnOrder(t *testing.T) {
	indexes := []connection.IndexDefinition{
		{Name: "PRIMARY", ColumnName: "device_id", SeqInIndex: 3, IndexType: "BTREE"},
		{Name: "idx_brand_device_date", ColumnName: "stat_date", SeqInIndex: 3, NonUnique: 1, IndexType: "BTREE"},
		{Name: "PRIMARY", ColumnName: "stat_date", SeqInIndex: 1, IndexType: "BTREE"},
		{Name: "idx_brand_device_date", ColumnName: "brand_id", SeqInIndex: 1, NonUnique: 1, IndexType: "BTREE"},
		{Name: "PRIMARY", ColumnName: "brand_id", SeqInIndex: 2, IndexType: "BTREE"},
		{Name: "idx_brand_device_date", ColumnName: "device_id", SeqInIndex: 2, NonUnique: 1, IndexType: "BTREE"},
	}
	rows := indexDisplayRows(indexes)
	if len(rows) != 2 || rows[0].name != "PRIMARY" || rows[0].columns != "stat_date, brand_id, device_id" || !rows[0].unique || rows[1].columns != "brand_id, device_id, stat_date" || rows[1].unique {
		t.Fatal(rows)
	}
	if indexes[0].ColumnName != "device_id" {
		t.Fatal("display mutated source metadata")
	}
	prefix := indexDisplayRows([]connection.IndexDefinition{{Name: "idx_prefix", ColumnName: "name", SubPart: 16}})
	if prefix[0].columns != "name(16)" {
		t.Fatal(prefix)
	}
}

func TestDesignerGroupedIndexSelectionTargetsWholeIndex(t *testing.T) {
	w, p := parityWindow(t)
	info, _ := (parityFixture{}).TableInfo(t.Context(), "main", "items")
	info.Indexes = []connection.IndexDefinition{
		{Name: "PRIMARY", ColumnName: "id", SeqInIndex: 1},
		{Name: "PRIMARY", ColumnName: "name", SeqInIndex: 2},
		{Name: "idx_name", ColumnName: "name", SeqInIndex: 1, NonUnique: 1},
	}
	w.designTable(p, domain.Object{Name: "items", Kind: "table"}, info)
	d := w.designers[w.tabs.Selected()]
	d.removeIndexes(map[int]bool{1: true}, false)
	if len(d.indexChanges) != 1 || d.indexChanges[0].OriginalName != "idx_name" {
		t.Fatal("grouped row targeted wrong index", d.indexChanges)
	}
	d.removeIndexes(map[int]bool{1: true}, true)
	if len(d.indexChanges) != 0 {
		t.Fatal("undo did not target grouped index", d.indexChanges)
	}
}
