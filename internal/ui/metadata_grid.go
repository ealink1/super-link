package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (w *Window) metadataGrid(result domain.Result, info domain.TableInfo) fyne.CanvasObject {
	table := widget.NewTable(func() (int, int) { return len(result.Rows), len(result.Columns) + 1 }, func() fyne.CanvasObject {
		label := widget.NewLabel("")
		label.Wrapping = fyne.TextTruncate
		return label
	}, func(id widget.TableCellID, item fyne.CanvasObject) {
		label := item.(*widget.Label)
		if id.Col == 0 {
			label.SetText(fmt.Sprint(id.Row + 1))
		} else {
			label.SetText(previewValue(result.Rows[id.Row][id.Col-1]))
		}
	})
	table.ShowHeaderRow = true
	table.CreateHeader = func() fyne.CanvasObject {
		return container.NewVBox(widget.NewLabel(""), canvas.NewText("", theme.DisabledColor()))
	}
	table.UpdateHeader = func(id widget.TableCellID, item fyne.CanvasObject) {
		box := item.(*fyne.Container)
		label := box.Objects[0].(*widget.Label)
		kind := box.Objects[1].(*canvas.Text)
		if id.Col == 0 {
			label.SetText("#")
			kind.Text = ""
			kind.Refresh()
			return
		}
		if id.Col < 1 || id.Col > len(result.Columns) {
			return
		}
		column := result.Columns[id.Col-1]
		label.SetText(column.Name)
		kind.Text = ""
		kind.TextSize = 11
		kind.Color = theme.DisabledColor()
		for _, metadata := range info.Columns {
			if metadata.Name == column.Name {
				kind.Text = metadata.Type
				if metadata.Key == "PRI" {
					kind.Text = "PK · " + metadata.Type
					kind.Color = color.NRGBA{R: 217, G: 119, B: 6, A: 255}
				}
				break
			}
		}
		kind.Refresh()
	}
	table.SetColumnWidth(0, 42)
	for i := range result.Columns {
		table.SetColumnWidth(i+1, 180)
	}
	table.OnSelected = func(id widget.TableCellID) {
		if id.Col > 0 && id.Row >= 0 {
			w.showCell(result.Columns[id.Col-1].Name, result.Rows[id.Row][id.Col-1])
		}
	}
	return table
}

func (w *Window) showCell(name string, value any) {
	entry := widget.NewMultiLineEntry()
	entry.SetText(displayValue(value))
	entry.SetMinRowsVisible(12)
	copy := widget.NewButton("复制", func() { w.Window.Clipboard().SetContent(displayValue(value)) })
	d := newCardCustom(name, "关闭", container.NewBorder(nil, copy, nil, nil, entry), w.Window)
	d.Resize(fyne.NewSize(740, 480))
	d.Show()
}

func (w *Window) columnsView(info domain.TableInfo) fyne.CanvasObject {
	result := domain.Result{Columns: []domain.Column{{Name: "名称"}, {Name: "类型"}, {Name: "主键"}, {Name: "可为空"}, {Name: "默认值"}, {Name: "注释"}}}
	for _, c := range info.Columns {
		var value any
		if c.Default != nil {
			value = *c.Default
		}
		result.Rows = append(result.Rows, []any{c.Name, c.Type, c.Key, c.Nullable, value, c.Comment})
	}
	return w.resultView(result)
}
