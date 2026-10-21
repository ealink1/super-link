package ui

import (
	"testing"

	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestSplitAndJoinColumnType(t *testing.T) {
	cases := []struct {
		full   string
		base   string
		args   string
		suffix string
	}{
		{"VARCHAR(255)", "VARCHAR", "255", ""},
		{"DECIMAL(10,2)", "DECIMAL", "10,2", ""},
		{"INT(11) UNSIGNED", "INT", "11", "UNSIGNED"},
		{"INTEGER", "INTEGER", "", ""},
		{"TIMESTAMP WITH TIME ZONE", "TIMESTAMP WITH TIME ZONE", "", ""},
		{"", "", "", ""},
		{"BROKEN(", "BROKEN(", "", ""},
	}
	for _, entry := range cases {
		base, args, suffix := splitColumnType(entry.full)
		if base != entry.base || args != entry.args || suffix != entry.suffix {
			t.Fatalf("split %q = %q/%q/%q, want %q/%q/%q", entry.full, base, args, suffix, entry.base, entry.args, entry.suffix)
		}
		if rebuilt := joinColumnType(base, args, suffix); rebuilt != entry.full {
			t.Fatalf("join(%q,%q,%q) = %q, want %q", base, args, suffix, rebuilt, entry.full)
		}
	}
}

func TestColumnTypeArgsValidation(t *testing.T) {
	for _, value := range []string{"", "1", "255", "10,2", "10, 2"} {
		if !validColumnTypeArgs(value) {
			t.Fatalf("%q rejected", value)
		}
	}
	for _, value := range []string{"abc", "10,", "10,2,3", "-1", "10.2"} {
		if validColumnTypeArgs(value) {
			t.Fatalf("%q accepted", value)
		}
	}
}

func TestDesignerLengthColumnReassemblesType(t *testing.T) {
	d := &tableDesigner{
		status:   widget.NewLabel(""),
		selected: map[int]bool{},
		fields:   []designColumn{{column: connection.ColumnDefinition{Name: "name", Type: "VARCHAR(100)"}}},
	}
	if base, args := d.fieldValue(0, 1), d.fieldValue(0, 2); base != "VARCHAR" || args != "100" {
		t.Fatalf("列拆分错误: 类型=%v 长度=%v", base, args)
	}
	if err := d.editField(0, 2, "64", false); err != nil || d.fields[0].column.Type != "VARCHAR(64)" {
		t.Fatalf("改长度后类型=%q err=%v", d.fields[0].column.Type, err)
	}
	if err := d.editField(0, 2, "abc", false); err == nil {
		t.Fatal("非法长度被接受")
	}
	if d.fields[0].column.Type != "VARCHAR(64)" {
		t.Fatalf("非法长度污染类型: %q", d.fields[0].column.Type)
	}
	if err := d.editField(0, 1, "TEXT", false); err != nil || d.fields[0].column.Type != "TEXT" {
		t.Fatalf("切换无长度类型后=%q err=%v", d.fields[0].column.Type, err)
	}
	if err := d.editField(0, 1, "DECIMAL", false); err != nil || d.fields[0].column.Type != "DECIMAL(10,2)" {
		t.Fatalf("切换带默认长度类型后=%q err=%v", d.fields[0].column.Type, err)
	}
	if err := d.editField(0, 2, "18,4", false); err != nil || d.fields[0].column.Type != "DECIMAL(18,4)" {
		t.Fatalf("两位精度=%q err=%v", d.fields[0].column.Type, err)
	}
	if err := d.editField(0, 3, "true", false); err != nil || d.fields[0].column.Key != "PRI" {
		t.Fatalf("主键列索引错位 err=%v key=%q", err, d.fields[0].column.Key)
	}
	if err := d.editField(0, 5, "true", false); err != nil || d.fields[0].column.Nullable != "NO" {
		t.Fatalf("非空列索引错位 err=%v nullable=%q", err, d.fields[0].column.Nullable)
	}
	if err := d.editField(0, 7, "备注", false); err != nil || d.fields[0].column.Comment != "备注" {
		t.Fatalf("注释列索引错位 err=%v comment=%q", err, d.fields[0].column.Comment)
	}
}

func TestFieldTypeDropdownStagesSelectedType(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	value := "VARCHAR(64)"
	model := gridModel{columns: []domain.Column{{Name: "类型"}}, selected: map[int]bool{}, value: func(int, int) any { return value }, choices: map[int][]string{0: tableColumnTypes("mysql")}, edit: func(row, col int, text string, _ bool) error { value = text; return nil }}
	cell := newGridCell(model)
	cell.bind(widget.TableCellID{Row: 0, Col: 2})
	if !cell.choice.Visible() || cell.choice.Selected != "VARCHAR(64)" {
		t.Fatal("custom type not retained in dropdown")
	}
	cell.choice.SetSelected("BIGINT")
	if value != "BIGINT" || cell.choice.Selected != "BIGINT" {
		t.Fatal("dropdown did not stage type")
	}
	cell.bind(widget.TableCellID{Row: 1, Col: 2})
	if cell.choice.Selected != "BIGINT" {
		t.Fatal("recycled dropdown lost value")
	}
	model.edit = nil
	readonly := newGridCell(model)
	readonly.bind(widget.TableCellID{Row: 0, Col: 2})
	if readonly.choice != nil {
		t.Fatal("read-only type allocated an editable dropdown")
	}
}
