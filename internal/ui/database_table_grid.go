package ui

import (
	"fmt"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/domain"
	"image/color"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

var tableCatalogHeaders = []string{"名称", "行", "数据长度", "引擎", "创建日期", "修改日期", "排序规则", "注释"}

func (p *databaseTables) buildTableGrid() *widget.Table {
	p.catalogCells = make(map[*canvas.Rectangle]domain.Object)
	grid := widget.NewTable(func() (int, int) { return len(p.shown), len(tableCatalogHeaders) }, func() fyne.CanvasObject {
		row := &databaseTableRow{contextMenu: p.showTableMenu, selectRow: p.selectTableRow, beginSelection: p.beginCatalogSelection, dragSelection: p.dragCatalogSelection, endSelection: p.endCatalogSelection, hoverRow: p.hoverTableRow, open: func(object domain.Object) { p.owner.openTable(p.profile, object) }}
		row.ExtendBaseWidget(row)
		row.Wrapping = fyne.TextTruncate
		image := widget.NewIcon(icon("table"))
		return container.NewStack(canvas.NewRectangle(color.Transparent), container.NewBorder(nil, nil, image, nil, row))
	}, func(id widget.TableCellID, object fyne.CanvasObject) {
		stack := object.(*fyne.Container)
		background := stack.Objects[0].(*canvas.Rectangle)
		box := stack.Objects[1].(*fyne.Container)
		row := box.Objects[0].(*databaseTableRow)
		image := box.Objects[1].(*widget.Icon)
		row.object = p.shown[id.Row]
		row.rowIndex = id.Row
		p.catalogCells[background] = row.object
		background.FillColor = color.Transparent
		if p.selectedTables[row.object] {
			background.FillColor = theme.SelectionColor()
		} else if p.hoveredTable == row.object.Schema+"."+row.object.Name {
			background.FillColor = catalogHoverColor()
		}
		background.Refresh()
		row.Alignment = fyne.TextAlignLeading
		image.Hide()
		value := ""
		if id.Col == 0 {
			value = row.object.Name
			if row.object.Schema != "" {
				value = row.object.Schema + "." + value
			}
			image.Show()
		} else if stats := p.statistics[row.object.Name]; len(stats) == 7 {
			value = catalogValue(stats[id.Col-1])
			if id.Col == 2 && stats[1] != nil {
				value = catalogBytes(stats[1])
			}
		}
		row.SetText(value)
	})
	grid.OnSelected = func(id widget.TableCellID) {
		grid.UnselectAll()
		if id.Row >= 0 && id.Row < len(p.shown) {
			p.selectTableRow(p.shown[id.Row], terminalDesktopModifiers())
		}
	}
	grid.HideSeparators = true
	grid.ShowHeaderRow = true
	grid.CreateHeader = func() fyne.CanvasObject { return widget.NewLabel("") }
	grid.UpdateHeader = func(id widget.TableCellID, object fyne.CanvasObject) {
		if id.Col >= 0 && id.Col < len(tableCatalogHeaders) {
			object.(*widget.Label).SetText(tableCatalogHeaders[id.Col])
		}
	}
	for i, width := range []float32{280, 90, 120, 110, 180, 180, 180, 300} {
		grid.SetColumnWidth(i, width)
	}
	return grid
}

func catalogValue(value any) string {
	switch value := value.(type) {
	case nil:
		return ""
	case []byte:
		return string(value)
	case time.Time:
		return value.Format("2006-01-02 15:04:05")
	default:
		return fmt.Sprint(value)
	}
}
func catalogBytes(value any) string {
	bytes, err := strconv.ParseFloat(catalogValue(value), 64)
	if err != nil {
		return catalogValue(value)
	}
	units := []string{"B", "KB", "MB", "GB", "TB"}
	unit := 0
	for bytes >= 1024 && unit < len(units)-1 {
		bytes /= 1024
		unit++
	}
	if unit <= 1 {
		return fmt.Sprintf("%.0f %s", bytes, units[unit])
	}
	return fmt.Sprintf("%.1f %s", bytes, units[unit])
}

func (p *databaseTables) hoverTableRow(object domain.Object, inside bool) {
	previous := p.hoveredTable
	key := object.Schema + "." + object.Name
	if inside {
		p.hoveredTable = key
	} else if p.hoveredTable == key {
		p.hoveredTable = ""
	}
	if previous == p.hoveredTable {
		return
	}
	p.refreshCatalogHighlights()
}

// Zero column spacing keeps the row highlight continuous between cells.
// Inner padding remains unchanged so labels keep their text inset.
type catalogGridTheme struct{ fyne.Theme }

func (t catalogGridTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNamePadding {
		return 0
	}
	return t.Theme.Size(name)
}

func catalogHoverColor() color.Color {
	shade := color.NRGBAModel.Convert(theme.SelectionColor()).(color.NRGBA)
	shade.A /= 2
	return shade
}
