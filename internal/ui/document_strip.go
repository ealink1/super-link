package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"github.com/ealink1/super-link/internal/domain"
)

func (w *Window) buildDocuments() {
	w.docButtons = make(map[*container.TabItem]*documentTab)
	w.docStrip = container.NewHBox()
	w.docTooltip = newDocumentTooltip()
	strip := container.NewHScroll(w.docStrip)
	strip.SetMinSize(fyne.NewSize(0, 32))
	controls := container.NewHBox(action("", "add-row", w.newSelectedQuery), action("", "connection-menu", w.documentMenu))
	controls.Layout = &toolbarLayout{height: 32}
	w.docHeader = container.NewStack(container.NewBorder(nil, nil, nil, controls, strip))
	w.docBody = container.NewStack()
	w.docHost = container.NewStack(container.NewBorder(w.docHeader, nil, nil, nil, w.docBody), w.docTooltip.layer)
}

// Keep header widgets stable while switching, renaming or refreshing pages.
// Recreating their renderers keeps obsolete callback closures alive in Fyne's
// renderer cache; detach those callbacks as soon as a page leaves the strip.
func (w *Window) syncDocuments() {
	if w.docStrip == nil {
		return
	}
	w.docTooltip.hide()
	selected := w.tabs.Selected()
	buttons := make([]fyne.CanvasObject, 0, len(w.tabs.Items))
	active := make(map[*container.TabItem]bool, len(w.tabs.Items))
	for _, item := range w.tabs.Items {
		active[item] = true
		title, subtitle := w.documentLabel(item)
		button := w.docButtons[item]
		if button == nil {
			button = newDocumentTab(title, subtitle, item == selected, func() { w.tabs.Select(item) }, func() { w.closeTab(item) })
			button.tooltip = w.docTooltip
			w.docButtons[item] = button
		}
		button.title, button.subtitle, button.selected = title, subtitle, item == selected
		button.Refresh()
		buttons = append(buttons, button)
	}
	for item, button := range w.docButtons {
		if !active[item] {
			button.hideTooltip()
			button.selectTab, button.closeTab = nil, nil
			button.tooltip = nil
			delete(w.docButtons, item)
		}
	}
	w.docStrip.Objects = buttons
	w.docStrip.Refresh()
	if selected != nil {
		w.docBody.Objects = []fyne.CanvasObject{selected.Content}
	} else {
		w.docBody.Objects = nil
	}
	w.docBody.Refresh()
}

func (w *Window) documentLabel(item *container.TabItem) (title, subtitle string) {
	title = item.Text
	if page := w.databases[item]; page != nil {
		subtitle = documentContext(page.profile, page.scope)
	}
	if space := w.workspaces[item]; space != nil {
		title = "新建查询"
		if space.title != "" && space.title != space.profile.Name {
			title = space.title
		}
		subtitle = documentContext(space.profile, space.scope.Text)
	}
	if table := w.tables[item]; table != nil {
		title = table.object.Name
		subtitle = documentContext(table.profile, table.object.Scope)
	}
	if designer := w.designers[item]; designer != nil {
		subtitle = documentContext(designer.profile, designer.object.Scope)
	}
	if importer := w.imports[item]; importer != nil {
		subtitle = documentContext(importer.profile, importer.object.Scope)
	}
	return title, subtitle
}

func documentContext(profile domain.Profile, scope string) string {
	if scope == "" {
		scope = "未选择 / 默认"
	}
	return "连接：" + profile.Name + "\n数据库：" + scope
}
