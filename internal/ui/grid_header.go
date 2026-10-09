package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

type gridHeader struct {
	kindText string
	widget.BaseWidget
	model                 gridModel
	table                 *widget.Table
	name, kind            *canvas.Text
	check                 *widget.Check
	column                int
	divider               *canvas.Rectangle
	ascending, descending *widget.Button
	sortControls          *fyne.Container
}

func newGridHeader(model gridModel, table *widget.Table) *gridHeader {
	h := &gridHeader{model: model, table: table, name: canvas.NewText("", theme.ForegroundColor()), kind: canvas.NewText("", color.NRGBA{R: 65, G: 112, B: 164, A: 255}), check: widget.NewCheck("", nil)}
	h.ascending = widget.NewButtonWithIcon("", headerOutlineIcon("sort-up", `<path d="m6 15 6-6 6 6"/>`), func() { h.sort(false) })
	h.descending = widget.NewButtonWithIcon("", headerOutlineIcon("sort-down", `<path d="m6 9 6 6 6-6"/>`), func() { h.sort(true) })
	h.ascending.Importance, h.descending.Importance = widget.LowImportance, widget.LowImportance
	h.sortControls = container.NewWithoutLayout(container.NewThemeOverride(h.ascending, sortArrowTheme{}), container.NewThemeOverride(h.descending, sortArrowTheme{}))
	h.divider = canvas.NewRectangle(color.NRGBA{R: 128, G: 128, B: 128, A: 38})
	h.name.TextSize = 14
	h.name.TextStyle = fyne.TextStyle{Bold: true}
	h.kind.TextSize = 13

	h.ExtendBaseWidget(h)
	return h
}

func (h *gridHeader) bind(col int) {
	h.column = col
	h.check.Hide()
	h.sortControls.Hide()
	h.name.Show()
	h.kind.Show()
	h.kind.Text = ""
	h.kind.Color = color.NRGBA{R: 65, G: 112, B: 164, A: 255}
	switch {
	case col == 0:
		h.name.Hide()
		h.kind.Hide()
		h.check.Show()
		h.check.OnChanged = nil
		all := h.model.length() > 0
		for row := 0; row < h.model.length(); row++ {
			if !h.model.selected[row] {
				all = false
				break
			}
		}
		h.check.SetChecked(all)
		h.check.OnChanged = func(value bool) {
			if h.model.selectRow != nil {
				for row := 0; row < h.model.length(); row++ {
					h.model.selectRow(row, value)
				}
				h.table.Refresh()
			}
		}
	case col == 1:
		h.name.Text = "#"
	case col >= 2 && col < len(h.model.columns)+2:
		h.name.Text = h.model.columns[col-2].Name
		if h.model.sortColumn != nil {
			h.sortControls.Show()
			h.ascending.Importance, h.descending.Importance = widget.LowImportance, widget.LowImportance
			for _, sort := range h.model.sorts {
				if sort.Column == h.name.Text {
					if sort.Descending {
						h.descending.Importance = widget.HighImportance
					} else {
						h.ascending.Importance = widget.HighImportance
					}
					break
				}
			}
			h.ascending.Refresh()
			h.descending.Refresh()
		}
		for _, c := range h.model.info.Columns {
			if c.Name == h.name.Text {
				h.kind.Text = c.Type
				if gridNumericColumn(h.model, col-2) {
					h.kind.Text = "#  " + c.Type
				} else {
					h.kind.Text = "Abc  " + c.Type
				}
				break
			}
		}
	}
	h.kindText = h.kind.Text
	h.Refresh()
}
func (h *gridHeader) CreateRenderer() fyne.WidgetRenderer { return &gridHeaderRenderer{h: h} }

type gridHeaderRenderer struct{ h *gridHeader }

func (r *gridHeaderRenderer) MinSize() fyne.Size { return fyne.NewSize(28, 26) }
func (r *gridHeaderRenderer) Layout(size fyne.Size) {
	r.h.check.Resize(size)
	r.h.sortControls.Move(fyne.NewPos(max(0, size.Width-24), 5))
	r.h.sortControls.Resize(fyne.NewSize(20, 40))
	for index, control := range r.h.sortControls.Objects {
		control.Move(fyne.NewPos(0, float32(index)*20))
		control.Resize(fyne.NewSize(20, 20))
	}
	textWidth := max(0, size.Width-12)
	if r.h.sortControls.Visible() {
		textWidth = max(0, size.Width-34)
	}
	r.h.divider.Move(fyne.NewPos(size.Width-1, 6))
	r.h.divider.Resize(fyne.NewSize(1, max(0, size.Height-12)))
	nameY := float32(6)
	if r.h.column == 1 {
		nameY = 12
	}
	r.h.name.Move(fyne.NewPos(6, nameY))
	r.h.name.Resize(fyne.NewSize(textWidth, 18))
	r.h.kind.Move(fyne.NewPos(6, 28))
	r.h.kind.Resize(fyne.NewSize(textWidth, 18))
	r.h.kind.Text = fitText(r.h.kindText, textWidth, r.h.kind.TextSize, r.h.kind.TextStyle)
	if r.h.column >= 2 && r.h.column < len(r.h.model.columns)+2 {
		r.h.name.Text = fitText(r.h.model.columns[r.h.column-2].Name, textWidth, r.h.name.TextSize, r.h.name.TextStyle)
	}
}
func (r *gridHeaderRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.h.name, r.h.kind, r.h.check, r.h.divider, r.h.sortControls}
}
func (r *gridHeaderRenderer) Refresh() {
	r.h.name.Color = theme.ForegroundColor()
	r.h.kind.Color = color.NRGBA{R: 65, G: 112, B: 164, A: 255}
	r.Layout(r.h.Size())
	r.h.name.Refresh()
	r.h.kind.Refresh()
	r.h.check.Refresh()
}
func (r *gridHeaderRenderer) Destroy() {}

func (h *gridHeader) sort(descending bool) {
	if h.model.sortColumn == nil || h.column < 2 || h.column >= len(h.model.columns)+2 {
		return
	}
	if h.model.current != nil && !h.model.current() {
		return
	}
	h.model.sortColumn(h.model.columns[h.column-2].Name, descending)
}

type sortArrowTheme struct{ Theme }

func (t sortArrowTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInlineIcon:
		return 12
	case theme.SizeNameInnerPadding, theme.SizeNamePadding:
		return 0
	}
	return fyne.CurrentApp().Settings().Theme().Size(name)
}
func (t sortArrowTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	return fyne.CurrentApp().Settings().Theme().Color(name, variant)
}
