package ui

import (
	"fyne.io/fyne/v2"
	"github.com/ealink1/super-link/internal/domain"
)

func (p *databaseTables) tableMenu(object domain.Object) *fyne.Menu {
	if object.Scope == "" {
		object.Scope = p.scope
	}
	node := &navNode{kind: "object", profileID: p.profile.ID, scope: object.Scope, object: object}
	return p.owner.sidebar.objectMenu(node, p.refresh)
}

func (p *databaseTables) showTableMenu(object domain.Object, position fyne.Position) {
	showContextMenu(p.tableMenu(object), p.owner.Window.Canvas(), position)
}
