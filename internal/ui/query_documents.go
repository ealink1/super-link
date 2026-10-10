package ui

import (
	"context"
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/google/uuid"
)

func (s *workspace) renameQuery() {
	name := widget.NewEntry()
	name.SetText(s.title)
	dialog.ShowForm("重命名查询", "保存", "取消", []*widget.FormItem{widget.NewFormItem("名称", name)}, func(ok bool) {
		if !ok || s.closed || strings.TrimSpace(name.Text) == "" || len(name.Text) > 256 {
			return
		}
		s.title = strings.TrimSpace(name.Text)
		s.scheduleSave()
		s.owner.syncDocuments()
	}, s.owner.Window)
}

func (s *workspace) saveNamedQuery() {
	if s.closed || s.saving {
		return
	}
	name := widget.NewEntry()
	name.SetText(s.title)
	if s.title == "" || s.title == s.profile.Name {
		name.SetText("新建查询")
	}
	dialog.ShowForm("保存查询", "保存", "取消", []*widget.FormItem{widget.NewFormItem("名称", name)}, func(ok bool) {
		if !ok || s.closed || s.saving {
			return
		}
		query := domain.SavedQuery{ID: s.savedID, Revision: s.savedRevision, ProfileID: s.profile.ID, Title: name.Text, Scope: s.scope.Text, Schema: s.schemaName, Text: s.editor.Text}
		if query.ID == "" {
			query.ID = uuid.NewString()
		}
		s.persistQuery(query)
	}, s.owner.Window)
}

func (s *workspace) persistQuery(query domain.SavedQuery) {
	s.saving = true
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.owner.Store.SaveQuery(ctx, query) }, func(value any, err error) {
		s.saving = false
		if s.closed || s.profile.ID != query.ProfileID {
			return
		}
		if err != nil {
			s.owner.showError(err)
			return
		}
		saved := value.(domain.SavedQuery)
		s.savedID, s.savedRevision, s.title = saved.ID, saved.Revision, saved.Title
		s.status.SetText("已保存查询：" + saved.Title)
		if s.editor.Text != saved.Text || s.scope.Text != saved.Scope || s.schemaName != saved.Schema {
			s.status.SetText(s.status.Text + " · 当前编辑还有未保存的改动")
		}
		s.scheduleSave()
		s.owner.syncDocuments()
	})
}

func (w *Window) savedQueryManager() {
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Store.SavedQueries(ctx, "") }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		queries := value.([]domain.SavedQuery)
		selected := -1
		profiles := map[string]domain.Profile{}
		for _, p := range w.profiles {
			profiles[p.ID] = p
		}
		list := widget.NewList(func() int { return len(queries) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(i int, object fyne.CanvasObject) {
			q := queries[i]
			object.(*widget.Label).SetText(fmt.Sprintf("%s · %s · %s", q.Title, profiles[q.ProfileID].Name, q.Scope))
		})
		status := widget.NewLabel("选择已存查询后打开；查询内容不会自动执行。")
		open := action("打开查询", "sql-doc", nil)
		remove := action("删除", "trash", nil)
		open.Disable()
		remove.Disable()
		closed := false
		modal := dialog.NewCustom("全部已存查询", "关闭", container.NewBorder(nil, container.NewVBox(status, container.NewHBox(open, remove)), nil, nil, list), w.Window)
		modal.SetOnClosed(func() { closed = true })
		list.OnSelected = func(i int) { selected = i; open.Enable(); remove.Enable() }
		open.OnTapped = func() {
			if selected < 0 || selected >= len(queries) {
				return
			}
			queryID := queries[selected].ID
			open.Disable()
			w.jobs.run(func(ctx context.Context) (any, error) { return w.Store.SavedQuery(ctx, queryID) }, func(value any, err error) {
				if closed {
					return
				}
				open.Enable()
				if err != nil {
					w.showError(err)
					return
				}
				if w.openSavedQuery(value.(domain.SavedQuery)) != nil {
					modal.Hide()
				}
			})
		}
		remove.OnTapped = func() {
			if selected < 0 || selected >= len(queries) {
				return
			}
			q := queries[selected]
			showConfirmDialog("删除已存查询", "删除「"+q.Title+"」？已打开的编辑草稿仍会保留。", func(ok bool) {
				if !ok || closed {
					return
				}
				w.jobs.run(func(ctx context.Context) (any, error) { return nil, w.Store.DeleteQuery(ctx, q.ID, q.Revision) }, func(_ any, err error) {
					if closed {
						return
					}
					if err != nil {
						w.showError(err)
						return
					}
					modal.Hide()
					w.savedQueryManager()
				})
			}, w.Window)
		}
		modal.Resize(fyne.NewSize(780, 520))
		modal.Show()
	})
}

func (w *Window) openSavedQuery(query domain.SavedQuery) *workspace {
	for _, p := range w.profiles {
		if p.ID != query.ProfileID {
			continue
		}
		s := w.openWorkspace(p, domain.Draft{ID: uuid.NewString(), ProfileID: p.ID, Title: query.Title, Text: query.Text, Scope: query.Scope, Schema: query.Schema})
		if s != nil {
			s.savedID, s.savedRevision = query.ID, query.Revision
			s.refreshObjects()
		}
		return s
	}
	w.showError(domain.ErrNotFound)
	return nil
}
