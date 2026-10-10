package ui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"github.com/google/uuid"
)

func (n *navigator) nodeProfile(node *navNode) (domain.Profile, bool) {
	for _, profile := range n.owner.profiles {
		if profile.ID == node.profileID {
			return profile, true
		}
	}
	n.owner.showError(domain.ErrNotFound)
	return domain.Profile{}, false
}

func (n *navigator) nodeMenu(node *navNode) *fyne.Menu {
	if node.kind == "connection-group" {
		return fyne.NewMenu("连接分组", fyne.NewMenuItem("管理连接分组", n.owner.groupManager), fyne.NewMenuItem("刷新", n.owner.reload))
	}
	query := fyne.NewMenuItem("新建查询", func() { n.queryForNode(node) })
	refresh := fyne.NewMenuItem("刷新", func() { n.selected = node.id; n.refresh() })
	if node.kind != "object" {
		menu := fyne.NewMenu("连接", query, refresh, fyne.NewMenuItem("编辑连接", func() { n.owner.selected = node.profileID; n.owner.editSelected() }), fyne.NewMenuItem("断开连接", func() { n.owner.selected = node.profileID; n.owner.disconnectSelected() }))
		if node.kind == "connection" {
			menu.Items = append(menu.Items[:1], append([]*fyne.MenuItem{n.createDatabaseItem(node)}, menu.Items[1:]...)...)
		}
		if node.kind == "database" || node.kind == "schema" || node.kind == "category" && strings.HasSuffix(node.id, "/table") {
			index := 1
			menu.Items = append(menu.Items[:index], append([]*fyne.MenuItem{n.createTableItem(node)}, menu.Items[index:]...)...)
		}
		if node.kind == "category" && strings.HasSuffix(node.id, "/table") {
			menu.Items = append(menu.Items, fyne.NewMenuItem("导入数据", func() { n.tableTransfer(node, true) }))
		}
		if node.kind == "database" {
			menu.Items = append(menu.Items, fyne.NewMenuItem("执行 SQL 文件", func() { n.executeSQLFile(node) }), fyne.NewMenuItem("导出全部表结构 · SQL", func() { n.exportDatabase(node, false) }), fyne.NewMenuItem("备份全部表 · 结构 + 数据 SQL", func() { n.exportDatabase(node, true) }))
		}
		return menu
	}
	return n.objectMenu(node, func() { n.selected = node.id; n.refresh() })
}

// objectMenu is shared by the navigator tree and database table list.
func (n *navigator) objectMenu(node *navNode, refreshAction func()) *fyne.Menu {
	query := fyne.NewMenuItem("新建查询", func() { n.queryForNode(node) })
	refresh := fyne.NewMenuItem("刷新", refreshAction)
	menu := fyne.NewMenu("对象", fyne.NewMenuItem("查看数据", func() { n.openObject(node) }), query, fyne.NewMenuItem("复制名称", func() { fyne.CurrentApp().Clipboard().SetContent(node.object.Name) }), refresh)
	p, ok := n.nodeProfile(node)
	if !ok {
		return menu
	}
	descriptor, _ := domain.Resolve(p.Config.Type)
	if descriptor.Family != domain.SQL {
		return menu
	}
	design := fyne.NewMenuItem("设计表", func() { n.withObjectInfo(node, n.owner.designTable) })
	design.Disabled = node.object.Kind == "view"
	menu.Items = append(menu.Items[:1], append([]*fyne.MenuItem{design, fyne.NewMenuItem("DDL", func() {
		n.withObjectInfo(node, func(_ domain.Profile, object domain.Object, info domain.TableInfo) {
			modal := dialog.NewCustom("DDL · "+object.Name, "关闭", ddlView(info.DDL), n.owner.Window)
			modal.Resize(fyne.NewSize(900, 560))
			modal.Show()
		})
	})}, menu.Items[1:]...)...)
	menu.Items = append(menu.Items, fyne.NewMenuItem("复制结构", func() {
		n.withObjectInfo(node, func(_ domain.Profile, _ domain.Object, info domain.TableInfo) {
			fyne.CurrentApp().Clipboard().SetContent(info.DDL)
		})
	}), fyne.NewMenuItem("复制 INSERT 模板", func() {
		n.withObjectInfo(node, func(p domain.Profile, object domain.Object, info domain.TableInfo) {
			text, err := insertTemplate(p.SQLDialect(), object, info)
			if err != nil {
				n.owner.showError(err)
				return
			}
			fyne.CurrentApp().Clipboard().SetContent(text)
		})
	}), fyne.NewMenuItem("导出数据", func() {
		n.withObjectInfo(node, n.exportObject)
	}))
	menu.Items = append(menu.Items, n.tableMutationItems(node, p)...)
	return menu
}

func (n *navigator) withObjectInfo(node *navNode, use func(domain.Profile, domain.Object, domain.TableInfo)) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	object := node.object
	name := object.Name
	var err error
	if object.Schema != "" {
		name, err = sqlworkbench.ObjectName(p.SQLDialect(), object)
	}
	if err != nil {
		n.owner.showError(err)
		return
	}
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		current, err := n.owner.Profiles.Get(ctx, p.ID)
		if err != nil {
			return nil, err
		}
		if current.Revision != p.Revision {
			return nil, domain.ErrConflict
		}
		return n.owner.Engine.TableInfo(ctx, p.ID, object.Scope, name)
	}, func(value any, err error) {
		if err != nil {
			n.owner.showError(err)
			return
		}
		use(p, object, value.(domain.TableInfo))
	})
}

func (n *navigator) queryForNode(node *navNode) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	schema := node.object.Schema
	for ancestor := node; ancestor != nil; ancestor = n.nodes[ancestor.parent] {
		if ancestor.kind == "schema" {
			schema = ancestor.label
			break
		}
	}
	scope := node.scope
	if node.kind == "connection" {
		scope = p.Config.Database
		if p.SQLDialect() == "oracle" {
			scope = ""
		}
	}
	s := n.owner.openWorkspace(p, domain.Draft{ID: uuid.NewString(), ProfileID: p.ID, Text: "", Scope: scope, Schema: schema})
	if s == nil {
		return
	}
	s.editor.SetText(s.descriptor.DefaultQuery())
	if p.SQLDialect() == "oracle" {
		s.editor.SetText("SELECT 1 FROM dual;")
	}
	if node.kind == "object" {
		s.selectedObject = &node.object
		s.previewObject()
	}
	if scope == "" {
		s.loadScopes()
	} else {
		s.refreshObjects()
	}
}

func (n *navigator) exportObject(p domain.Profile, object domain.Object, info domain.TableInfo) {
	query, err := sqlworkbench.BuildExportQuery(p.SQLDialect(), domain.TableRequest{Object: object}, info)
	if err != nil {
		n.owner.showError(err)
		return
	}
	columns := []domain.Column{}
	for _, c := range info.Columns {
		columns = append(columns, domain.Column{Name: c.Name})
	}
	name, _ := sqlworkbench.ObjectName(p.SQLDialect(), object)
	n.owner.exportDialog(exportSource{profile: p, result: domain.Result{Columns: columns}, request: domain.Execution{Revision: p.Revision, Scope: object.Scope, Text: query}, table: name})
}

func insertTemplate(dialect string, object domain.Object, info domain.TableInfo) (string, error) {
	name, err := sqlworkbench.ObjectName(dialect, object)
	if err != nil {
		return "", err
	}
	columns, values := []string{}, []string{}
	for _, c := range info.Columns {
		if !domain.WritableColumn(c) || strings.Contains(strings.ToLower(c.Extra), "auto_increment") {
			continue
		}
		column, err := sqlworkbench.Quote(dialect, c.Name)
		if err != nil {
			return "", err
		}
		columns = append(columns, column)
		values = append(values, fmt.Sprintf(":value_%d", len(values)+1))
	}
	if len(columns) == 0 {
		return "", fmt.Errorf("table has no writable columns for an INSERT template")
	}
	return "INSERT INTO " + name + " (" + strings.Join(columns, ", ") + ")\nVALUES (" + strings.Join(values, ", ") + ");", nil
}
