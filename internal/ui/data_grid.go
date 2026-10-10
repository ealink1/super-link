package ui

import (
	"encoding/base64"
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

type gridModel struct {
	sortColumn   func(string, bool)
	sorts        []domain.Sort
	editorDrafts map[widget.TableCellID]gridEditorDraft
	canEditCell  func(int, int) bool
	columns      []domain.Column
	info         domain.TableInfo
	length       func() int
	value        func(int, int) any
	selected     map[int]bool
	selectRow    func(int, bool)
	edit         func(int, int, string, bool) error
	inspect      func(int, int)
	copyValue    func(string)
	changed      func(int) string
	choices      map[int][]string
	boolColumns  map[int]bool
	pending      func()
	current      func() bool
}

// dataGrid keeps Fyne's virtualization and header resizing.
type dataGrid struct {
	*widget.Table
	model          gridModel
	cells          []*gridCell
	rowNumberWidth float32
}

func newDataGrid(model gridModel) *dataGrid {
	model.editorDrafts = make(map[widget.TableCellID]gridEditorDraft)
	g := &dataGrid{model: model}
	g.Table = widget.NewTable(func() (int, int) { return model.length(), len(model.columns) + 2 }, func() fyne.CanvasObject { return newGridCell(model) }, func(id widget.TableCellID, item fyne.CanvasObject) {
		cell := item.(*gridCell)
		if !cell.tracked {
			cell.tracked = true
			g.cells = append(g.cells, cell)
		}
		cell.bind(id)
	})
	g.ExtendBaseWidget(g)
	g.ShowHeaderRow = true
	g.HideSeparators = true
	// Fyne skips visible trailing columns when custom-width columns are sticky.
	// Keep all columns in the same scroll surface until that calculation is fixed.
	g.StickyColumnCount = 0
	g.CreateHeader = func() fyne.CanvasObject { return newGridHeader(model, g.Table) }
	g.UpdateHeader = func(id widget.TableCellID, item fyne.CanvasObject) { item.(*gridHeader).bind(id.Col) }
	g.SetRowHeight(-1, 52)
	g.SetColumnWidth(0, 28)
	for i := range model.columns {
		g.SetColumnWidth(i+2, 140)
	}
	g.Refresh()
	return g
}
func (g *dataGrid) Refresh() {
	digits := max(2, len(strconv.Itoa(max(1, g.model.length()))))
	width := float32(40)
	for _, digit := range "0123456789" {
		width = max(width, fyne.MeasureText(strings.Repeat(string(digit), digits), 14, fyne.TextStyle{}).Width+20)
	}
	if width != g.rowNumberWidth {
		g.rowNumberWidth = width
		g.SetColumnWidth(1, width)
	}
	g.Table.Refresh()
}
func (g *dataGrid) Resize(size fyne.Size) {
	if len(g.model.columns) > 0 {
		width := size.Width - 28 - g.rowNumberWidth - float32(len(g.model.columns)-1)*144 - 16
		g.SetColumnWidth(len(g.model.columns)+1, max(140, width))
	}
	g.Table.Resize(size)
}

type gridCell struct {
	widget.BaseWidget
	model        gridModel
	id           widget.TableCellID
	text         *canvas.Text
	check        *widget.Check
	choice       *widget.Select
	entry        *gridEditEntry
	background   *canvas.Rectangle
	divider      *canvas.Rectangle
	editing      bool
	fullText     string
	originalText string
	tracked      bool
	preview      gridTextPreview
}

func newGridCell(model gridModel) *gridCell {
	c := &gridCell{model: model, text: canvas.NewText("", theme.ForegroundColor()), check: widget.NewCheck("", nil), entry: newGridEditEntry(), background: canvas.NewRectangle(color.Transparent)}
	c.text.TextSize = 14
	c.divider = canvas.NewRectangle(color.NRGBA{R: 128, G: 128, B: 128, A: 38})
	c.choice = widget.NewSelect(nil, nil)
	c.choice.Hide()
	c.entry.Hide()
	c.check.Hide()
	c.ExtendBaseWidget(c)
	c.entry.OnSubmitted = func(string) { _ = c.commitEditor() }
	c.entry.onBlur = func() { _ = c.commitEditor() }
	c.entry.OnChanged = func(string) {
		if c.editing {
			c.rememberEditor()
			if c.model.pending != nil {
				c.model.pending()
			}
		}
	}
	return c
}

func (c *gridCell) bind(id widget.TableCellID) {
	if c.id != id {
		_ = c.commitEditor()
		c.editing = false
		c.entry.Hide()
	}
	c.id = id
	c.check.Hide()
	c.choice.Hide()
	c.text.Show()
	c.text.Color = theme.ForegroundColor()
	c.background.FillColor = color.Transparent
	if id.Row%2 == 1 {
		c.background.FillColor = color.NRGBA{R: 128, G: 128, B: 128, A: 8}
	}
	c.text.Alignment = fyne.TextAlignLeading
	if id.Col == 1 || id.Col >= 2 && gridNumericColumn(c.model, id.Col-2) {
		c.text.Alignment = fyne.TextAlignTrailing
	}
	if id.Col == 0 {
		c.check.Enable()
		c.text.Hide()
		c.check.Show()
		c.check.OnChanged = nil
		c.check.SetChecked(c.model.selected[id.Row])
		c.check.OnChanged = func(value bool) {
			if c.model.selectRow != nil {
				c.model.selectRow(c.id.Row, value)
			}
		}
	} else if id.Col == 1 {
		c.text.Text = fmt.Sprint(id.Row + 1)
	} else {
		value := c.model.value(id.Row, id.Col-2)
		if c.model.boolColumns[id.Col-2] {
			c.text.Hide()
			c.check.Show()
			c.check.OnChanged = nil
			checked, _ := value.(bool)
			c.check.SetChecked(checked)
			if c.model.edit == nil {
				c.check.Disable()
			} else {
				c.check.Enable()
				c.check.OnChanged = func(v bool) {
					if err := c.model.edit(c.id.Row, c.id.Col-2, fmt.Sprint(v), false); err != nil {
						c.bind(c.id)
					}
				}
			}
		} else {
			c.check.Enable()
		}
		c.text.Text = previewValue(value)
		if draft, ok := c.model.editorDrafts[id]; ok {
			c.text.Text = draft.text
			c.text.Color = theme.ErrorColor()
		}
		if value == nil {
			c.text.Color = theme.DisabledColor()
		}
		if c.model.changed != nil {
			switch c.model.changed(id.Row) {
			case "insert":
				c.background.FillColor = color.NRGBA{R: 21, G: 128, B: 61, A: 20}
			case "update":
				c.background.FillColor = color.NRGBA{R: 245, G: 158, B: 11, A: 24}
			case "delete":
				c.background.FillColor = color.NRGBA{R: 220, G: 38, B: 38, A: 20}
				c.text.Color = theme.DisabledColor()
			}
		}
	}
	if c.id.Col >= 2 && c.model.edit != nil && len(c.model.choices[c.id.Col-2]) > 0 && !c.editing {
		c.choice.OnChanged = nil
		c.choice.Options = append([]string{}, c.model.choices[c.id.Col-2]...)
		current := displayValue(c.model.value(c.id.Row, c.id.Col-2))
		found := false
		for _, option := range c.choice.Options {
			if option == current {
				found = true
				break
			}
		}
		if !found && current != "" {
			c.choice.Options = append(c.choice.Options, current)
		}
		c.choice.Options = append(c.choice.Options, "自定义…")
		c.choice.SetSelected(current)
		c.choice.OnChanged = func(value string) {
			if c.model.current != nil && !c.model.current() {
				return
			}
			if value == "自定义…" {
				c.DoubleTapped(nil)
				return
			}
			if err := c.model.edit(c.id.Row, c.id.Col-2, value, false); err != nil {
				c.entry.SetValidationError(err)
			}
			c.bind(c.id)
		}
		c.text.Hide()
		c.choice.Show()
	}
	if c.editing {
		c.text.Hide()
		c.choice.Hide()
	}

	c.fullText = c.text.Text
	c.Refresh()
}
func (c *gridCell) DoubleTapped(*fyne.PointEvent) {
	if c.id.Col < 2 || c.model.boolColumns[c.id.Col-2] {
		return
	}
	if c.model.edit == nil || c.model.canEditCell != nil && !c.model.canEditCell(c.id.Row, c.id.Col-2) {
		if c.model.inspect != nil {
			c.model.inspect(c.id.Row, c.id.Col-2)
		}
		return
	}
	if c.editing {
		fyne.CurrentApp().Driver().CanvasForObject(c).Focus(c.entry)
		return
	}
	c.editing = false
	value := c.model.value(c.id.Row, c.id.Col-2)
	text := ""
	if value != nil {
		text = displayValue(value)
		if binary, ok := value.([]byte); ok {
			text = base64.StdEncoding.EncodeToString(binary)
		}
	}
	c.originalText = text
	if draft, ok := c.model.editorDrafts[c.id]; ok {
		c.originalText, text = draft.original, draft.text
	}
	c.entry.SetText(text)
	c.editing = true
	c.text.Hide()
	c.choice.Hide()
	c.entry.Show()
	fyne.CurrentApp().Driver().CanvasForObject(c).Focus(c.entry)
}
func (c *gridCell) TappedSecondary(event *fyne.PointEvent) {
	if c.id.Col < 2 {
		return
	}
	menu := fyne.NewMenu("单元格", fyne.NewMenuItem("查看完整值", func() {
		if c.model.inspect != nil {
			c.model.inspect(c.id.Row, c.id.Col-2)
		}
	}), fyne.NewMenuItem("复制", func() {
		if c.model.copyValue != nil {
			c.model.copyValue(displayValue(c.model.value(c.id.Row, c.id.Col-2)))
		}
	}))
	if c.model.edit != nil && (c.model.canEditCell == nil || c.model.canEditCell(c.id.Row, c.id.Col-2)) {
		menu.Items = append(menu.Items, fyne.NewMenuItem("设为 NULL", func() { _ = c.model.edit(c.id.Row, c.id.Col-2, "", true); c.bind(c.id) }))
	}
	widget.ShowPopUpMenuAtPosition(menu, fyne.CurrentApp().Driver().CanvasForObject(c), event.AbsolutePosition)
}
func (c *gridCell) CreateRenderer() fyne.WidgetRenderer { return &gridCellRenderer{c: c} }

type gridCellRenderer struct{ c *gridCell }

func (r *gridCellRenderer) MinSize() fyne.Size { return fyne.NewSize(32, 28) }
func (r *gridCellRenderer) Layout(size fyne.Size) {
	r.c.background.Resize(size)
	r.c.divider.Move(fyne.NewPos(size.Width-1, 0))
	r.c.divider.Resize(fyne.NewSize(1, size.Height))
	r.c.check.Resize(size)
	r.c.entry.Resize(size)
	r.c.choice.Resize(size)
	r.c.text.Move(fyne.NewPos(8, (size.Height-r.c.text.MinSize().Height)/2))
	// Theme color changes do not change glyph widths. Reuse the clipped preview
	// instead of shaping long values again on every layout and color refresh.
	r.c.text.Text = r.c.preview.fit(r.c.fullText, max(0, size.Width-16), r.c.text.TextSize, r.c.text.TextStyle)
	r.c.text.Resize(fyne.NewSize(max(0, size.Width-16), r.c.text.MinSize().Height))
}
func (r *gridCellRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.c.background, r.c.divider, r.c.text, r.c.check, r.c.choice, r.c.entry}
}
func (r *gridCellRenderer) Refresh() {
	r.Layout(r.c.Size())
	r.c.background.Refresh()
	r.c.text.Refresh()
	r.c.check.Refresh()
}
func (r *gridCellRenderer) Destroy() {}

func gridNumericColumn(model gridModel, index int) bool {
	if index < 0 || index >= len(model.columns) {
		return false
	}
	for _, column := range model.info.Columns {
		if column.Name != model.columns[index].Name {
			continue
		}
		kind := strings.ToLower(column.Type)
		for _, prefix := range []string{"int", "tinyint", "smallint", "mediumint", "bigint", "decimal", "numeric", "float", "double", "real", "number", "serial"} {
			if strings.HasPrefix(kind, prefix) {
				return true
			}
		}
	}
	return false
}
