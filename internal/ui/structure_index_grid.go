package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
)

// Use the same grid selection and cell inspection without a per-column row
// number: each row now represents an entire index, as in the index list design.
func newIndexListGrid(model gridModel) *widget.Table {
	mappedColumn := func(column int) int {
		if column == 0 {
			return 0
		}
		return column + 1
	}
	table := widget.NewTable(func() (int, int) { return model.length(), len(model.columns) + 1 }, func() fyne.CanvasObject {
		return newGridCell(model)
	}, func(id widget.TableCellID, object fyne.CanvasObject) {
		cell := object.(*gridCell)
		cell.bind(widget.TableCellID{Row: id.Row, Col: mappedColumn(id.Col)})
		cell.text.TextStyle.Bold = id.Col == 2 || id.Col == 4
		if id.Col == 4 && model.value(id.Row, 3) == "唯一" {
			cell.text.Color = color.NRGBA{R: 205, G: 116, B: 16, A: 255}
		}
		cell.text.Refresh()
	})
	table.ShowHeaderRow = true
	table.HideSeparators = true
	table.CreateHeader = func() fyne.CanvasObject { return newGridHeader(model, table) }
	table.UpdateHeader = func(id widget.TableCellID, object fyne.CanvasObject) {
		object.(*gridHeader).bind(mappedColumn(id.Col))
	}
	table.SetRowHeight(-1, 36)
	for column, width := range []float32{36, 270, 360, 120, 100, 90} {
		table.SetColumnWidth(column, width)
	}
	return table
}
