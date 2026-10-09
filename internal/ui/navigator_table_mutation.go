package ui

import (
	"context"
	"errors"
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"time"
)

func (n *navigator) tableMutationItems(node *navNode, p domain.Profile) []*fyne.MenuItem {
	items := []*fyne.MenuItem{fyne.NewMenuItemSeparator()}
	for _, operation := range []struct{ key, label string }{{"drop", "删除表"}, {"clear", "清空表"}, {"truncate", "截断表"}} {
		key, label := operation.key, operation.label
		sql, err := sqlworkbench.TableMutationSQL(p.SQLDialect(), node.object, key)
		item := fyne.NewMenuItem(label, func() {
			request := domain.Execution{Revision: p.Revision, Scope: node.object.Scope, Schema: node.object.Schema, Text: sql, Write: true, MaxRows: 1}
			if key != "clear" {
				request.Action = "structure"
			}
			n.submitTableMutation(p, node.object, key, label, request)
		})
		item.Disabled = err != nil || p.ReadOnly || (key == "clear" && p.Config.Protection.RestrictScriptExecution) || (key != "clear" && p.Config.Protection.RestrictStructureEdit)
		items = append(items, item)
	}
	return items
}

func (n *navigator) submitTableMutation(p domain.Profile, object domain.Object, key, label string, request domain.Execution) {
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return n.owner.Engine.Execute(ctx, p.ID, request)
	}, func(_ any, err error) {
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			detail := "将删除表结构及全部数据。"
			if key == "clear" {
				detail = "将使用 DELETE 删除全部数据，保留表结构。"
			}
			if key == "truncate" {
				detail = "将使用 TRUNCATE 清除全部数据，保留表结构；自增计数的处理取决于数据库。"
			}
			dialog.ShowConfirm(label, fmt.Sprintf("连接：%s\n表：%s / %s / %s\n%s此操作无法在应用中撤销。\n\n%s", p.Name, object.Scope, object.Schema, object.Name, detail, request.Text), func(ok bool) {
				if ok {
					request.Confirmation = required.Fingerprint
					n.submitTableMutation(p, object, key, label, request)
				}
			}, n.owner.Window)
			return
		}
		if err != nil {
			n.owner.showError(fmt.Errorf("%s失败：%w", label, err))
			return
		}
		n.refreshTableMutationViews(p, object, key)
	})
}

func (n *navigator) refreshTableMutationViews(p domain.Profile, object domain.Object, key string) {
	// Refresh each view once; repeated refresh cancels its in-flight query.
	n.refresh()
	for _, page := range n.owner.databases {
		if page.profile.ID == p.ID && page.scope == object.Scope {
			page.refresh()
		}
	}
	n.refreshMutatedTable(p, object, key)
}

func (n *navigator) refreshMutatedTable(p domain.Profile, object domain.Object, key string) {
	for _, table := range n.owner.tables {
		if table.profile.ID == p.ID && table.object.Scope == object.Scope && table.object.Schema == object.Schema && table.object.Name == object.Name {
			if key == "drop" {
				n.owner.closeTable(table)
			} else {
				table.refresh()
			}
		}
	}
}
