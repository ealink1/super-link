package ui

import (
	"context"
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"time"
)

func (n *navigator) tableTransfer(node *navNode, importing bool) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	title := "导出数据"
	if importing {
		title = "导入数据"
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
		return n.owner.Engine.Objects(ctx, p.ID, node.scope)
	}, func(value any, err error) {
		if err != nil {
			n.owner.showError(err)
			return
		}
		objects := []domain.Object{}
		labels := []string{}
		for _, object := range value.([]domain.Object) {
			if object.Kind != "table" {
				continue
			}
			if object.Scope == "" {
				object.Scope = node.scope
			}
			label := object.Name
			if object.Schema != "" {
				label = object.Schema + "." + label
			}
			objects = append(objects, object)
			labels = append(labels, fmt.Sprintf("%d · %s", len(objects), label))
		}
		if len(objects) == 0 {
			dialog.ShowInformation(title, "当前数据库没有可用的数据表。", n.owner.Window)
			return
		}
		selected := 0
		picker := widget.NewSelect(labels, func(label string) {
			for index, candidate := range labels {
				if candidate == label {
					selected = index
					break
				}
			}
		})
		picker.SetSelectedIndex(0)
		modal := dialog.NewForm(title+" · "+node.scope, "下一步", "取消", []*widget.FormItem{widget.NewFormItem("数据表", picker)}, func(ok bool) {
			if !ok {
				return
			}
			target := &navNode{kind: "object", profileID: p.ID, scope: node.scope, object: objects[selected]}
			if importing {
				n.withObjectInfo(target, n.owner.importTable)
			} else {
				n.withObjectInfo(target, n.exportObject)
			}
		}, n.owner.Window)
		modal.Resize(fyne.NewSize(520, 180))
		modal.Show()
	})
}
