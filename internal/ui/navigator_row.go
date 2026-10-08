package ui

import (
	"github.com/ealink1/super-link/internal/application"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

type treeRow struct {
	widget.BaseWidget
	navigator *navigator
	node      *navNode
	image     *databaseBadge
	label     *widget.Label
	status    *connectionDot
}

func newTreeRow(n *navigator) *treeRow {
	r := &treeRow{navigator: n, image: newDatabaseBadge("folder"), label: widget.NewLabel(""), status: newConnectionDot()}
	r.status.Hide()
	r.label.Wrapping = fyne.TextTruncate
	r.ExtendBaseWidget(r)
	return r
}
func (r *treeRow) bind(node *navNode) {
	r.node = node
	if node == nil {
		r.label.SetText("")
		r.status.Hide()
		return
	}
	if node.kind == "connection" {
		status := r.navigator.connectionStatuses[node.profileID]
		r.status.setStatus(status)
		if status == application.ConnectionDisconnected {
			r.status.Hide()
		} else {
			r.status.Show()
		}
	} else {
		r.status.Hide()
	}
	r.label.SetText(node.label)
	name := "folder"
	if node.kind == "database" {
		name = "database"
	}
	if node.kind == "object" {
		name = "table"
		if node.object.Kind == "view" {
			name = "view"
		}
	}
	if node.kind == "category" {
		parts := strings.Split(node.id, "/")
		name = parts[len(parts)-1]
	}
	if node.kind == "connection" {
		for _, p := range r.navigator.owner.profiles {
			if p.ID == node.profileID {
				name = "db-" + p.Config.Type
				if p.IconType != "" {
					name = "db-" + p.IconType
				}
				break
			}
		}
	}
	r.image.set(name)
	if node.kind != "connection" {
		shade := "#15803d"
		if node.kind == "database" {
			shade = "#4286a5"
		} else if node.kind == "schema" {
			shade = "#89918b"
		}
		r.image.image.Resource = coloredIcon(name, shade)
	}
	if node.kind == "connection" {
		for _, p := range r.navigator.owner.profiles {
			if p.ID == node.profileID && p.IconColor != "" {
				r.image.frame.StrokeColor = hexColor(p.IconColor)
				r.image.Refresh()
				break
			}
		}
	}
	r.Refresh()
}
func (r *treeRow) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(container.NewBorder(nil, nil, r.image, r.status, r.label))
}
func (r *treeRow) Tapped(*fyne.PointEvent) {
	if r.node != nil {
		r.navigator.tree.Select(r.node.id)
	}
}
func (r *treeRow) DoubleTapped(*fyne.PointEvent) {
	if r.node != nil {
		r.navigator.activate(r.node.id)
	}
}
func (r *treeRow) TappedSecondary(event *fyne.PointEvent) {
	if r.node == nil {
		return
	}
	r.navigator.tree.Select(r.node.id)
	node := r.node
	menu := r.navigator.nodeMenu(node)
	showContextMenu(menu, r.navigator.owner.Window.Canvas(), event.AbsolutePosition)
}
