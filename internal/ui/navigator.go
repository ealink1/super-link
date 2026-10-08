package ui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
)

type navNode struct {
	parent                            string
	generation                        uint64
	id, label, kind, profileID, scope string
	object                            domain.Object
	children                          []string
	loaded, busy                      bool
	revision                          int64
	cancel                            context.CancelFunc
}

type navigator struct {
	owner              *Window
	tree               *widget.Tree
	nodes              map[string]*navNode
	roots              []string
	breadcrumb         *widget.Label
	contextLabel       *widget.Label
	selected           string
	kindFilter         string
	connectionStatuses map[string]application.ConnectionStatus
}

func newNavigator(w *Window) *navigator {
	n := &navigator{owner: w, nodes: make(map[string]*navNode), breadcrumb: widget.NewLabel("连接")}
	n.breadcrumb.Wrapping = fyne.TextTruncate
	n.breadcrumb.TextStyle = fyne.TextStyle{Bold: true}
	n.contextLabel = widget.NewLabel("")
	n.contextLabel.TextStyle = fyne.TextStyle{Monospace: true}
	n.contextLabel.Wrapping = fyne.TextTruncate
	n.contextLabel.Hide()
	n.owner.search.Hide()
	n.tree = widget.NewTree(func(id string) []string {
		if id == "" {
			return n.roots
		}
		if node := n.nodes[id]; node != nil {
			return n.filteredChildren(node)
		}
		return nil
	},
		func(id string) bool {
			if id == "" {
				return true
			}
			node := n.nodes[id]
			return node != nil && node.kind != "object" && node.kind != "message"
		},
		func(bool) fyne.CanvasObject { return newTreeRow(n) },
		func(id string, _ bool, item fyne.CanvasObject) { item.(*treeRow).bind(n.nodes[id]) })
	n.tree.HideSeparators = true
	n.tree.OnBranchOpened = n.expand
	n.tree.OnSelected = n.selectNode
	n.watchConnectionStatuses()
	return n
}

func (n *navigator) content() fyne.CanvasObject {
	tools := container.NewHBox(action("", "search", func() {
		if n.owner.search.Visible() {
			n.owner.search.SetText("")
			n.owner.search.Hide()
		} else {
			n.owner.search.Show()
			n.owner.Window.Canvas().Focus(n.owner.search)
		}
	}), action("", "locate", n.locate), action("", "refresh", n.refresh), action("", "connection-menu", n.connectionMenu))
	setFilter := func(kind string) func() { return func() { n.kindFilter = kind; n.tree.Refresh() } }
	filters := container.NewHBox(action("", "all-objects", setFilter("")), action("", "table", setFilter("table")), action("", "view", setFilter("view")), action("", "function", setFilter("function")), action("", "sql-doc", n.owner.savedQueryManager), action("", "history", n.owner.sqlTools))
	head := container.NewVBox(container.NewBorder(nil, nil, nil, tools, n.breadcrumb), n.contextLabel, n.owner.search, filters)
	footer := container.NewVBox(action("全部已存查询", "folder-open", n.owner.savedQueryManager))
	return container.NewBorder(head, footer, nil, nil, n.tree)
}

func (n *navigator) syncProfiles() {
	current := make(map[string]bool, len(n.owner.profiles))
	for _, p := range n.owner.profiles {
		current["connection:"+p.ID] = true
	}
	for id, node := range n.nodes {
		if node.kind == "connection" && !current[id] {
			n.removeChildren(node)
			if node.cancel != nil {
				node.cancel()
			}
			delete(n.nodes, id)
		}
	}
	n.roots = n.roots[:0]
	for _, p := range n.owner.visible {
		id := "connection:" + p.ID
		node := n.nodes[id]
		if node == nil || node.revision != p.Revision {
			if node != nil {
				n.removeChildren(node)
				if node.cancel != nil {
					node.cancel()
				}
			}
			node = &navNode{id: id, label: p.Name, kind: "connection", profileID: p.ID, revision: p.Revision}
			n.nodes[id] = node
		}
		n.roots = append(n.roots, id)
	}
	n.tree.Refresh()
}

func (n *navigator) selectNode(id string) {
	node := n.nodes[id]
	if node == nil {
		return
	}
	n.owner.selected = node.profileID
	n.selected = id
	for _, p := range n.owner.profiles {
		if p.ID == node.profileID {
			n.breadcrumb.SetText(p.Name)
			break
		}
	}
	n.contextLabel.SetText(node.scope)
	if node.scope == "" {
		n.contextLabel.Hide()
	} else {
		n.contextLabel.Show()
	}
	if node.kind == "database" {
		if p, ok := n.nodeProfile(node); ok {
			n.owner.openDatabaseTables(p, node.scope)
		}
	}
}

func (n *navigator) expand(id string) {
	node := n.nodes[id]
	if node == nil || node.loaded || node.busy {
		return
	}
	if node.kind == "category" {
		return
	}
	node.busy = true
	node.generation++
	generation := node.generation
	n.addMessage(node, "正在加载…")
	n.owner.jobs.runWithCancel(&node.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		if node.kind == "connection" {
			return n.owner.Engine.Scopes(ctx, node.profileID)
		}
		return n.owner.Engine.Objects(ctx, node.profileID, node.scope)
	}, func(value any, err error) {
		if n.nodes[id] != node || generation != node.generation {
			return
		}
		node.busy = false
		node.children = nil
		if err != nil {
			n.addMessage(node, "加载失败："+err.Error())
			return
		}
		node.loaded = true
		if node.kind == "connection" {
			for i, scope := range value.([]string) {
				label := scope
				if label == "" {
					label = "默认范围"
				}
				child := &navNode{id: fmt.Sprintf("%s/db/%d", id, i), label: label, kind: "database", profileID: node.profileID, scope: scope}
				child.parent = id
				n.nodes[child.id] = child
				node.children = append(node.children, child.id)
			}
		} else {
			n.objectChildren(node, value.([]domain.Object))
		}
		if len(node.children) == 0 {
			n.addMessage(node, "没有可见对象")
		}
		n.tree.Refresh()
	})
}

func (n *navigator) addMessage(node *navNode, label string) {
	id := node.id + "/state"
	n.nodes[id] = &navNode{id: id, label: label, kind: "message", profileID: node.profileID, scope: node.scope}
	n.nodes[id].parent = node.id
	node.children = []string{id}
	n.tree.Refresh()
}

func (n *navigator) objectChildren(parent *navNode, objects []domain.Object) {
	groups := make(map[string]*navNode)
	schemas := make(map[string]*navNode)
	for i, object := range objects {
		objectParent := parent
		if object.Schema != "" {
			schema := schemas[object.Schema]
			if schema == nil {
				schema = &navNode{id: fmt.Sprintf("%s/schema/%d", parent.id, len(schemas)), label: object.Schema, kind: "schema", profileID: parent.profileID, scope: parent.scope, loaded: true}
				schemas[object.Schema] = schema
				schema.parent = parent.id
				n.nodes[schema.id] = schema
				parent.children = append(parent.children, schema.id)
			}
			objectParent = schema
		}
		kind := object.Kind
		if kind == "sql" {
			kind = "table"
		}
		key := objectParent.id + "/" + kind
		group := groups[key]
		if group == nil {
			labels := map[string]string{"table": "表", "view": "视图", "document": "集合", "cache": "Key", "message": "Topic / Queue", "configuration": "配置", "vector": "Collection", "search": "索引"}
			label := labels[kind]
			if label == "" {
				label = "对象"
			}
			group = &navNode{id: key, label: label, kind: "category", profileID: parent.profileID, scope: parent.scope, loaded: true}
			groups[key] = group
			group.parent = objectParent.id
			n.nodes[group.id] = group
			objectParent.children = append(objectParent.children, group.id)
		}
		object.Scope = parent.scope
		leaf := &navNode{id: fmt.Sprintf("%s/%d", group.id, i), label: object.Name, kind: "object", profileID: parent.profileID, scope: parent.scope, object: object}
		n.nodes[leaf.id] = leaf
		leaf.parent = group.id
		group.children = append(group.children, leaf.id)
	}
	for _, group := range groups {
		group.label += fmt.Sprintf("  %d", len(group.children))
	}
}

func (n *navigator) activate(id string) {
	n.selectNode(id)
	node := n.nodes[id]
	if node == nil {
		return
	}
	if node.kind == "object" {
		n.openObject(node)
		return
	}
	if node.kind == "message" {
		if parent := n.nodes[id[:len(id)-6]]; parent != nil {
			parent.loaded = false
			n.expand(parent.id)
		}
		return
	}
	if n.tree.IsBranchOpen(id) {
		n.tree.CloseBranch(id)
	} else {
		n.tree.OpenBranch(id)
	}
}

func (n *navigator) openObject(node *navNode) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	d, _ := domain.Resolve(p.Config.Type)
	if d.Family == domain.SQL {
		n.owner.openTable(p, node.object)
		return
	}
	s := n.owner.openWorkspace(p, domain.Draft{})
	s.scope.SetText(node.scope)
	s.selectedObject = &node.object
	s.previewObject()
}

func (n *navigator) refresh() {
	id := n.selected
	for node := n.nodes[id]; node != nil && (node.kind == "object" || node.kind == "category" || node.kind == "schema" || node.kind == "message"); node = n.nodes[id] {
		if node.parent == "" {
			break
		}
		id = node.parent
	}
	if node := n.nodes[id]; node != nil {
		node.generation++
		if node.busy && node.cancel != nil {
			node.cancel()
		}
		n.removeChildren(node)
		node.busy = false
		node.loaded = false
		n.expand(id)
	}
}

func (n *navigator) removeChildren(node *navNode) {
	for _, id := range node.children {
		if child := n.nodes[id]; child != nil {
			n.removeChildren(child)
			if child.cancel != nil {
				child.cancel()
			}
			delete(n.nodes, id)
		}
	}
	node.children = nil
}
func (n *navigator) filteredChildren(node *navNode) []string {
	if n.kindFilter == "" {
		return node.children
	}
	children := make([]string, 0, len(node.children))
	for _, id := range node.children {
		child := n.nodes[id]
		if child == nil {
			continue
		}
		if child.kind == "category" && !strings.HasSuffix(child.id, "/"+n.kindFilter) {
			continue
		}
		if child.kind == "object" && child.object.Kind != n.kindFilter {
			continue
		}
		children = append(children, id)
	}
	return children
}
func (n *navigator) locate() {
	if id := n.selected; id != "" {
		n.tree.ScrollTo(id)
	}
}
func (n *navigator) connectionMenu() {
	menu := fyne.NewMenu("连接", fyne.NewMenuItem("新建查询", n.owner.newSelectedQuery), fyne.NewMenuItem("打开工作台", n.owner.openSelected), fyne.NewMenuItem("编辑连接", n.owner.editSelected), fyne.NewMenuItem("断开连接", n.owner.disconnectSelected), fyne.NewMenuItem("删除连接", n.owner.deleteSelected))
	widget.ShowPopUpMenuAtPosition(menu, n.owner.Window.Canvas(), fyne.NewPos(12, 90))
}
