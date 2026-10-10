package ui

import (
	"context"
	"errors"
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"time"
)

func (n *navigator) createDatabaseItem(node *navNode) *fyne.MenuItem {
	item := fyne.NewMenuItem("新建库", func() { n.createDatabase(node) })
	item.Disabled = true
	for _, p := range n.owner.profiles {
		if p.ID == node.profileID {
			item.Disabled = !sqlworkbench.CanCreateDatabase(p.SQLDialect())
			break
		}
	}
	return item
}

func (n *navigator) createDatabase(node *navNode) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	if p.ReadOnly {
		modal := newConfirmDialog("只读连接无法新建库", "当前连接开启了只读保护。新建库需要写入权限，请编辑连接并取消勾选“只读保护”后重试。是否打开编辑连接？", func(edit bool) {
			if edit {
				n.owner.editProfile(p)
			}
		}, n.owner.Window)
		modal.SetConfirmText("编辑连接")
		modal.Show()
		return
	}
	name := widget.NewEntry()
	name.SetPlaceHolder("输入库名")
	name.Validator = func(value string) error { _, err := sqlworkbench.CreateDatabaseSQL(p.SQLDialect(), value); return err }
	charset := widget.NewSelect([]string{"默认（服务器）"}, nil)
	charset.SetSelectedIndex(0)
	collation := widget.NewSelect([]string{"默认（服务器）"}, nil)
	collation.SetSelectedIndex(0)
	content := container.NewVBox(widget.NewLabel("* 数据库名称"), name, widget.NewLabel("字符集"), charset, widget.NewLabel("排序规则"), collation)
	modal := dialog.NewCustomConfirm("创建数据库", "确定", "取消", content, func(create bool) {
		if !create {
			return
		}
		cs, co := charset.Selected, collation.Selected
		if cs == "默认（服务器）" {
			cs = ""
		}
		if co == "默认（服务器）" {
			co = ""
		}
		sql, err := sqlworkbench.CreateDatabaseWithOptionsSQL(p.SQLDialect(), name.Text, cs, co)
		if err != nil {
			n.owner.showError(err)
			return
		}
		n.submitCreateDatabase(node, p, domain.Execution{Revision: p.Revision, Scope: p.Config.Database, Text: sql, Write: true, MaxRows: 1})
	}, n.owner.Window)
	modal.Resize(fyne.NewSize(520, 360))
	if sqlworkbench.DatabaseCharsetSupported(p.SQLDialect()) {
		n.loadDatabaseOptions(p, charset, collation)
	} else {
		charset.Disable()
		collation.Disable()
	}
	modal.Show()
	n.owner.Window.Canvas().Focus(name)
}

func (n *navigator) submitCreateDatabase(node *navNode, p domain.Profile, request domain.Execution) {
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return n.owner.Engine.Execute(ctx, p.ID, request)
	}, func(_ any, err error) {
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			showConfirmDialog("确认创建数据库", fmt.Sprintf("连接：%s\n操作：%s", p.Name, request.Text), func(ok bool) {
				if ok {
					request.Confirmation = required.Fingerprint
					n.submitCreateDatabase(node, p, request)
				}
			}, n.owner.Window)
			return
		}
		if err != nil {
			n.owner.showError(err)
			return
		}
		if n.nodes[node.id] == node {
			n.selected = node.id
			n.refresh()
		}
	})
}

func (n *navigator) loadDatabaseOptions(p domain.Profile, charset, collation *widget.Select) {
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 15*time.Second)
		defer cancel()
		return n.owner.Engine.Execute(ctx, p.ID, domain.Execution{Revision: p.Revision, Scope: p.Config.Database, Text: "SHOW COLLATION", MaxRows: 1024})
	}, func(value any, err error) {
		if err != nil {
			n.owner.showError(err)
			return
		}
		mapping := map[string][]string{}
		sets := []string{"默认（服务器）"}
		for _, result := range value.([]domain.Result) {
			for _, row := range result.Rows {
				if len(row) < 2 {
					continue
				}
				co, cs := databaseOptionText(row[0]), databaseOptionText(row[1])
				if cs == "" || co == "" {
					continue
				}
				if _, exists := mapping[cs]; !exists {
					sets = append(sets, cs)
				}
				mapping[cs] = append(mapping[cs], co)
			}
		}
		charset.Options = sets
		charset.OnChanged = func(cs string) {
			collation.Options = append([]string{"默认（服务器）"}, mapping[cs]...)
			collation.SetSelectedIndex(0)
			collation.Refresh()
		}
		charset.Refresh()
	})
}

func databaseOptionText(value any) string {
	if value == nil {
		return ""
	}
	if raw, ok := value.([]byte); ok {
		return string(raw)
	}
	return fmt.Sprint(value)
}
