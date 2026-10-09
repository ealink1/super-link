package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/domain"
	"image/color"
	"math"
)

type catalogSelectionDrag struct {
	anchor, last int
	base         map[domain.Object]bool
}

func cloneCatalogSelection(source map[domain.Object]bool) map[domain.Object]bool {
	target := make(map[domain.Object]bool, len(source))
	for object, selected := range source {
		if selected {
			target[object] = true
		}
	}
	return target
}

// Selection changes repaint only cached cell backgrounds. Labels, icons and
// the table layout do not need rebuilding when a highlight changes.
func (p *databaseTables) refreshCatalogHighlights() {
	for background, object := range p.catalogCells {
		var fill color.Color = color.Transparent
		if p.selectedTables[object] {
			fill = theme.SelectionColor()
		} else if p.hoveredTable == object.Schema+"."+object.Name {
			fill = catalogHoverColor()
		}
		if color.NRGBAModel.Convert(background.FillColor) != color.NRGBAModel.Convert(fill) {
			background.FillColor = fill
			background.Refresh()
		}
	}
}

func (p *databaseTables) beginCatalogSelection(object domain.Object, modifier fyne.KeyModifier) {
	p.dragSelection = nil
	for index, target := range p.shown {
		if target != object {
			continue
		}
		base := make(map[domain.Object]bool)
		if modifier&(fyne.KeyModifierControl|fyne.KeyModifierSuper) != 0 {
			base = cloneCatalogSelection(p.selectedTables)
		}
		p.dragSelection = &catalogSelectionDrag{anchor: index, last: index, base: base}
		p.selectTableRow(object, modifier)
		return
	}
}

func (p *databaseTables) dragCatalogSelection(index int) {
	drag := p.dragSelection
	if p.closed || drag == nil || len(p.shown) == 0 {
		return
	}
	index = max(0, min(index, len(p.shown)-1))
	if index == drag.last {
		return
	}
	drag.last = index
	next := cloneCatalogSelection(drag.base)
	for row := min(index, drag.anchor); row <= max(index, drag.anchor); row++ {
		next[p.shown[row]] = true
	}
	p.selectedTables = next
	p.refreshCatalogHighlights()
	p.updateTableSelectionStatus()
}

func (p *databaseTables) endCatalogSelection() { p.dragSelection = nil }

func (r *databaseTableRow) Dragged(event *fyne.DragEvent) {
	if r.dragSelection == nil || !r.mousePressed || r.Size().Height <= 0 {
		return
	}
	offset := int(math.Floor(float64((event.AbsolutePosition.Y - r.pressY + r.pressOffsetY) / r.Size().Height)))
	r.dragSelection(r.pressRowIndex + offset)
}
func (r *databaseTableRow) DragEnd() {
	if r.endSelection != nil {
		r.endSelection()
	}
}
