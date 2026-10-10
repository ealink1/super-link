package ui

import (
	"context"
	"errors"
	"fmt"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

func (d *tableDesigner) save(confirmation string) {
	if !d.stageEditors() {
		return
	}
	if !d.canEdit() || !d.dirty() {
		return
	}
	r := d.request()
	r.Confirmation = confirmation
	statements, err := sqlworkbench.BuildStructure(d.profile.SQLDialect(), r, d.info)
	if err != nil {
		d.status.SetText(err.Error())
		return
	}
	d.busy = true
	d.saveButton.Disable()
	d.status.SetText("正在保存结构变更…")
	d.owner.jobs.runWithCancel(&d.cancel, func(ctx context.Context) (any, error) { return d.owner.Engine.ApplyStructure(ctx, d.profile.ID, r) }, func(value any, err error) {
		if d.closed {
			return
		}
		d.busy = false
		d.saveButton.Enable()
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			text := fmt.Sprintf("连接：%s · %s\n表：%s · %d 条 SQL\nMySQL DDL 会逐条生效；中断时需要检查已完成的变更。", d.profile.Name, d.profile.Environment, d.object.Name, len(statements))
			content := container.NewBorder(widget.NewLabel(text), nil, nil, nil, ddlView(sqlworkbench.PreviewStructure(statements)))
			modal := dialog.NewCustomConfirm("保存结构变更", "执行", "取消", content, func(ok bool) {
				if ok {
					d.save(required.Fingerprint)
				}
			}, d.owner.Window)
			modal.Resize(fyne.NewSize(920, 650))
			modal.Show()
			return
		}
		result := domain.StructureResult{}
		if value != nil {
			result = value.(domain.StructureResult)
		}
		var warning *domain.CommittedWarning
		if err != nil && !errors.As(err, &warning) {
			d.uncertain = errors.Is(err, domain.ErrUnknownOutcome) || result.Applied > 0
			if d.uncertain {
				d.saveButton.Disable()
			}
			d.status.SetText(fmt.Sprintf("已确认完成 %d 条 SQL · %s", result.Applied, err.Error()))
			return
		}
		d.reset(d.info)
		d.reload()
		for _, t := range d.owner.tables {
			if t.profile.ID == d.profile.ID && t.object == d.object && !t.dirty() {
				t.refresh()
			}
		}
		if warning != nil {
			showInformationDialog("结构变更已提交", warning.Error(), d.owner.Window)
		}
	})
}
func (d *tableDesigner) refresh() {
	if d.busy || d.closed {
		return
	}
	if d.dirty() {
		showConfirmDialog("刷新结构", "丢弃当前暂存修改并重新读取服务端结构？", func(ok bool) {
			if ok {
				d.reload()
			}
		}, d.owner.Window)
		return
	}
	d.reload()
}
func (d *tableDesigner) reload() {
	d.busy = true
	d.status.SetText("正在读取最新结构…")
	p, object := d.profile, d.object
	name, err := sqlworkbench.ObjectName(p.SQLDialect(), object)
	if err != nil {
		d.busy = false
		d.status.SetText(err.Error())
		return
	}
	if object.Schema == "" {
		name = object.Name
	}
	d.owner.jobs.runWithCancel(&d.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return d.owner.Engine.TableInfo(ctx, p.ID, object.Scope, name)
	}, func(value any, err error) {
		if d.closed {
			return
		}
		d.busy = false
		if p.Revision != d.profile.Revision {
			d.status.SetText("连接配置已改变，请重新刷新结构。")
			return
		}
		if err != nil {
			d.status.SetText(err.Error())
			return
		}
		d.reset(value.(domain.TableInfo))
		d.views.Items[1].Content = d.indexPanel()
		d.views.Items[2].Content = d.owner.foreignKeysView(d.info)
		d.views.Items[3].Content = d.owner.triggersView(d.info)
		d.views.Items[4].Content = ddlView(d.info.DDL)
		d.views.Refresh()
		if d.canEdit() {
			d.saveButton.Enable()
		}
		d.status.SetText("已读取最新结构")
	})
}
func (w *Window) closeDesigner(d *tableDesigner) {
	close := func() {
		d.closed = true
		if d.cancel != nil {
			d.cancel()
		}
		delete(w.designers, d.item)
		w.tabs.Remove(d.item)
		w.syncDocuments()
	}
	if d.dirty() || d.busy {
		showConfirmDialog("关闭设计表", "丢弃暂存修改并关闭？进行中的操作会取消，已执行的 DDL 可能保留。", func(ok bool) {
			if ok {
				close()
			}
		}, w.Window)
		return
	}
	close()
}
