package ui

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func (n *navigator) createTableItem(node *navNode) *fyne.MenuItem {
	item := fyne.NewMenuItem("新建表", func() { n.createTable(node) })
	item.Icon = icon("table")
	item.Disabled = true
	for _, p := range n.owner.profiles {
		if p.ID == node.profileID {
			item.Disabled = !sqlworkbench.CanCreateTable(p.SQLDialect())
			break
		}
	}
	return item
}

func (n *navigator) tableTarget(node *navNode, p domain.Profile) domain.Object {
	object := domain.Object{Kind: "table", Scope: node.scope, Schema: node.object.Schema}
	if node.kind == "connection" {
		object.Scope = p.Config.Database
	}
	for ancestor := node; ancestor != nil; ancestor = n.nodes[ancestor.parent] {
		if ancestor.kind == "schema" {
			object.Schema = ancestor.label
			break
		}
	}
	return object
}

func (n *navigator) createTable(node *navNode) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	if p.ReadOnly || p.Config.Protection.RestrictStructureEdit {
		n.owner.showError(fmt.Errorf("当前连接限制结构修改，请编辑连接后取消只读或结构修改保护"))
		return
	}
	target := n.tableTarget(node, p)
	d := &tableDesigner{owner: n.owner, profile: p, object: target, selected: map[int]bool{}, body: container.NewStack(), status: widget.NewLabel("双击单元格编辑字段；默认值使用 SQL 字面量，例如 '文本'。")}
	d.fields = []designColumn{{column: connection.ColumnDefinition{Name: "id", Type: "INTEGER", Key: "PRI", Nullable: "NO"}}}
	d.rebuildFields()
	name := widget.NewEntry()
	name.SetPlaceHolder("输入新表名称")
	build := func() (string, error) {
		if !d.stageEditors() {
			return "", fmt.Errorf("请先修正字段输入")
		}
		target.Name = strings.TrimSpace(name.Text)
		columns := []connection.ColumnDefinition{}
		for _, f := range d.fields {
			if !f.deleted {
				columns = append(columns, f.column)
			}
		}
		return sqlworkbench.CreateTableSQL(p.SQLDialect(), target, columns)
	}
	preview := widget.NewButton("SQL 预览", func() {
		sql, err := build()
		if err != nil {
			d.status.SetText(err.Error())
			return
		}
		modal := dialog.NewCustom("建表 SQL", "关闭", ddlView(sql), n.owner.Window)
		modal.Resize(fyne.NewSize(760, 460))
		modal.Show()
	})
	tools := container.NewHBox(widget.NewButton("新增字段", func() {
		if d.stageEditors() {
			d.addField()
		}
	}), widget.NewButton("删除字段", func() {
		if d.stageEditors() {
			d.deleteFields()
		}
	}), preview)
	var modal *dialog.CustomDialog
	create := widget.NewButton("创建表", func() {
		if d.busy || d.closed {
			return
		}
		sql, err := build()
		if err != nil {
			d.status.SetText(err.Error())
			return
		}
		d.busy = true
		d.status.SetText("正在创建表…")
		n.submitCreateTable(node, p, domain.Execution{Revision: p.Revision, Scope: target.Scope, Schema: target.Schema, Text: sql, Write: true, Action: "structure", MaxRows: 1}, func(err error) {
			if d.closed {
				return
			}
			d.busy = false
			if err != nil {
				d.status.SetText(err.Error())
				return
			}
			modal.Hide()
		})
	})
	header := container.NewVBox(widget.NewLabel("目标："+p.Name+" / "+target.Scope+" / "+target.Schema), name, tools)
	content := container.NewBorder(header, container.NewVBox(d.status, create), nil, nil, d.body)
	modal = dialog.NewCustom("新建表", "关闭", content, n.owner.Window)
	modal.SetOnClosed(func() { d.closed = true; d.grid.discardEditors() })
	modal.Resize(fyne.NewSize(960, 540))
	modal.Show()
	n.owner.Window.Canvas().Focus(name)
}

func (n *navigator) submitCreateTable(node *navNode, p domain.Profile, request domain.Execution, done func(error)) {
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return n.owner.Engine.Execute(ctx, p.ID, request)
	}, func(_ any, err error) {
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			showConfirmDialog("确认创建表", fmt.Sprintf("连接：%s\n%s", p.Name, request.Text), func(ok bool) {
				if !ok {
					done(fmt.Errorf("已取消创建表"))
					return
				}
				request.Confirmation = required.Fingerprint
				n.submitCreateTable(node, p, request, done)
			}, n.owner.Window)
			return
		}
		if err == nil && n.nodes[node.id] == node {
			n.selected = node.id
			n.refresh()
		}
		done(err)
	})
}
