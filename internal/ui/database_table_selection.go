package ui

import (
	"fmt"
	"fyne.io/fyne/v2"
	"github.com/ealink1/super-link/internal/domain"
)

func (p *databaseTables) selectTableRow(object domain.Object, modifier fyne.KeyModifier) {
	if p.closed {
		return
	}
	toggle := modifier&(fyne.KeyModifierControl|fyne.KeyModifierSuper) != 0
	if !toggle || p.selectedTables == nil {
		p.selectedTables = make(map[domain.Object]bool)
	}
	if toggle && p.selectedTables[object] {
		delete(p.selectedTables, object)
	} else {
		p.selectedTables[object] = true
	}
	p.refreshCatalogHighlights()
	p.updateTableSelectionStatus()
}

func (p *databaseTables) pruneTableSelection() {
	visible := make(map[domain.Object]bool, len(p.shown))
	for _, object := range p.shown {
		visible[object] = true
	}
	for object := range p.selectedTables {
		if !visible[object] {
			delete(p.selectedTables, object)
		}
	}
}

func (p *databaseTables) tableSelectionNotice() string {
	if len(p.selectedTables) == 0 {
		return ""
	}
	return fmt.Sprintf(" · 已选 %d 张表", len(p.selectedTables))
}
