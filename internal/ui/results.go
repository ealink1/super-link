package ui

import (
	"encoding/json"
	"fmt"
	"strings"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (w *Window) resultView(result domain.Result) fyne.CanvasObject {
	if len(result.Columns) == 0 {
		return widget.NewLabel(fmt.Sprintf("受影响行：%d\n%s", result.RowsAffected, strings.Join(result.Messages, "\n")))
	}
	table := widget.NewTable(func() (int, int) { return len(result.Rows), len(result.Columns) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(id widget.TableCellID, object fyne.CanvasObject) {
		label := object.(*widget.Label)
		label.SetText(previewValue(result.Rows[id.Row][id.Col]))
		label.Wrapping = fyne.TextTruncate
	})
	table.ShowHeaderRow = true
	table.CreateHeader = func() fyne.CanvasObject {
		return widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	}
	table.UpdateHeader = func(id widget.TableCellID, object fyne.CanvasObject) {
		if id.Row == -1 && id.Col >= 0 && id.Col < len(result.Columns) {
			object.(*widget.Label).SetText(result.Columns[id.Col].Name)
		} else {
			object.(*widget.Label).SetText("")
		}
	}
	for i := range result.Columns {
		table.SetColumnWidth(i, 180)
	}
	table.OnSelected = func(id widget.TableCellID) {
		value := displayValue(result.Rows[id.Row][id.Col])
		entry := widget.NewMultiLineEntry()
		entry.SetText(value)
		entry.SetMinRowsVisible(12)
		entry.Wrapping = fyne.TextWrapWord
		copy := widget.NewButton("复制完整单元格", func() { w.Window.Clipboard().SetContent(value) })
		d := newCardCustom(result.Columns[id.Col].Name, "关闭", container.NewBorder(nil, copy, nil, nil, entry), w.Window)
		d.Resize(fyne.NewSize(740, 480))
		d.Show()
	}
	note := fmt.Sprintf("%d 行 · %d 列 · %s", len(result.Rows), len(result.Columns), result.Duration.Round(1000000))
	if result.Truncated {
		note += " · 已达到预览上限（结果不完整）"
	}
	return container.NewBorder(nil, widget.NewLabel(note), nil, nil, table)
}
func displayValue(value any) string {
	if value == nil {
		return "NULL"
	}
	switch v := value.(type) {
	case string:
		return v
	case []byte:
		if utf8.Valid(v) {
			return string(v)
		}
		return fmt.Sprintf("0x%x", v)
	default:
		raw, err := json.MarshalIndent(v, "", "  ")
		if err == nil {
			return string(raw)
		}
		return fmt.Sprint(v)
	}
}
func previewValue(value any) string {
	valueText := strings.ReplaceAll(displayValue(value), "\n", " ↵ ")
	runes := []rune(valueText)
	if len(runes) > 160 {
		return string(runes[:160]) + "…"
	}
	return valueText
}
