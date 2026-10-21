package ui

import (
	"context"
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (w *Window) draftManager() {
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Store.Drafts(ctx) }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		drafts := value.([]domain.Draft)
		profiles := make(map[string]domain.Profile)
		for _, profile := range w.profiles {
			profiles[profile.ID] = profile
		}
		list := widget.NewList(func() int { return len(drafts) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(id widget.ListItemID, item fyne.CanvasObject) {
			draft := drafts[id]
			status := "已打开"
			if draft.Closed {
				status = "已关闭"
			}
			item.(*widget.Label).SetText(fmt.Sprintf("%s · %s · %s", profiles[draft.ProfileID].Name, status, previewValue(draft.Text)))
		})
		modal := newCardCustom("本地草稿 · 单击重新打开", "关闭", list, w.Window)
		list.OnSelected = func(id widget.ListItemID) {
			draft := drafts[id]
			for item, space := range w.workspaces {
				if space.id == draft.ID {
					w.tabs.Select(item)
					modal.Hide()
					return
				}
			}
			if profile, exists := profiles[draft.ProfileID]; exists {
				w.openWorkspace(profile, draft)
				modal.Hide()
			}
		}
		modal.Resize(fyne.NewSize(900, 500))
		modal.Show()
	})
}
