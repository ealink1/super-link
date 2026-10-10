package ui

import (
	"strconv"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestDataGridRowNumbersRemainVisibleAsRowsGrow(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	rows := 9
	grid := newDataGrid(gridModel{length: func() int { return rows }})
	for _, count := range []int{9, 10, 99, 100, 1000, 10000} {
		rows = count
		grid.Refresh()
		cell := grid.CreateCell().(*gridCell)
		grid.UpdateCell(widget.TableCellID{Row: count - 1, Col: 1}, cell)
		cell.Resize(fyne.NewSize(grid.rowNumberWidth, 28))
		test.WidgetRenderer(cell).Layout(cell.Size())
		if cell.text.Text != strconv.Itoa(count) {
			t.Fatalf("row %d rendered as %q", count, cell.text.Text)
		}
	}
}
