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
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

func (p *databaseTables) buildDeleteButton() *widget.Button {
	button := widget.NewButtonWithIcon("", theme.DeleteIcon(), p.deleteSelectedTables)
	button.Importance = widget.LowImportance
	return button
}

func (p *databaseTables) updateDeleteButton() {
	if p.deleteButton == nil {
		return
	}
	enabled := !p.closed && !p.deleting && len(p.selectedTables) > 0 && len(p.selectedTables) <= 1000 && !p.profile.ReadOnly && !p.profile.Config.Protection.RestrictStructureEdit
	for object := range p.selectedTables {
		if _, err := sqlworkbench.TableMutationSQL(p.profile.SQLDialect(), object, "drop"); err != nil {
			enabled = false
			break
		}
	}
	if enabled {
		p.deleteButton.Enable()
	} else {
		p.deleteButton.Disable()
	}
}

func (p *databaseTables) deletionTargets() []domain.Object {
	objects := make([]domain.Object, 0, len(p.selectedTables))
	for _, object := range p.shown {
		if p.selectedTables[object] {
			objects = append(objects, object)
		}
	}
	return objects
}

func (p *databaseTables) deleteSelectedTables() {
	if p.closed || p.deleting {
		return
	}
	objects := p.deletionTargets()
	if len(objects) == 0 {
		return
	}
	p.deleting = true
	p.updateDeleteButton()
	p.submitDeleteTables(objects, "")
}

func (p *databaseTables) submitDeleteTables(objects []domain.Object, confirmation string) {
	p.owner.jobs.runWithCancel(&p.deleteCancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 10*time.Minute)
		defer cancel()
		return p.owner.Engine.DropTables(ctx, p.profile.ID, p.profile.Revision, objects, confirmation, func(done, total int) {
			fyne.Do(func() {
				if !p.closed {
					p.status.SetText(fmt.Sprintf("正在删除表：%d / %d", done, total))
				}
			})
		})
	}, func(value any, err error) {
		if p.closed {
			return
		}
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			names := make([]string, 0, len(objects))
			for _, object := range objects {
				name := object.Name
				if object.Schema != "" {
					name = object.Schema + "." + name
				}
				names = append(names, name)
			}
			list := widget.NewLabel(strings.Join(names, "\n"))
			content := container.NewBorder(widget.NewLabel(fmt.Sprintf("连接：%s / %s\n将删除以下 %d 张表的结构和全部数据，无法在应用中撤销。", p.profile.Name, p.scope, len(objects))), nil, nil, nil, container.NewVScroll(list))
			modal := dialog.NewCustomConfirm("删除选中的表", "删除", "取消", content, func(ok bool) {
				if p.closed {
					return
				}
				if ok {
					p.status.SetText(fmt.Sprintf("正在删除表：0 / %d", len(objects)))
					p.submitDeleteTables(objects, required.Fingerprint)
				} else {
					p.deleting = false
					p.updateTableSelectionStatus()
				}
			}, p.owner.Window)
			modal.Resize(fyne.NewSize(560, 420))
			modal.Show()
			return
		}
		p.deleting = false
		completed, _ := value.([]domain.Object)
		for _, object := range completed {
			delete(p.selectedTables, object)
			p.owner.sidebar.refreshMutatedTable(p.profile, object, "drop")
		}
		p.updateDeleteButton()
		if len(completed) > 0 {
			p.owner.sidebar.refresh()
			for _, page := range p.owner.databases {
				if page.profile.ID == p.profile.ID && page.scope == p.scope {
					page.refresh()
				}
			}
		} else {
			p.updateTableSelectionStatus()
		}
		if err != nil {
			p.owner.showError(err)
		}
	})
}
