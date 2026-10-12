package ui

import (
	"slices"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
)

func (w *Window) documentTabMenu(anchor *container.TabItem) *fyne.Menu {
	menu := w.tabs.closeMenu(anchor, func(group tabCloseGroup) { w.closeDocumentTabs(anchor, group) })
	if w.closingDocuments {
		for _, item := range menu.Items {
			item.Disabled = true
		}
	}
	return menu
}

func (w *Window) closeDocumentTabs(anchor *container.TabItem, group tabCloseGroup) {
	if w.closingDocuments || w.shuttingDown {
		return
	}
	items := w.tabs.tabsToClose(anchor, group)
	if len(items) == 0 {
		return
	}
	w.closingDocuments = true
	w.closeNextDocument(items)
}

// Wait for confirmation and draft persistence before moving to the next page.
// This keeps a single dialog visible and stops the batch on cancel or failure.
func (w *Window) closeNextDocument(items []*container.TabItem) {
	for len(items) > 0 && !slices.Contains(w.tabs.Items, items[0]) {
		items[0], items = nil, items[1:]
	}
	if len(items) == 0 || w.shuttingDown {
		w.closingDocuments = false
		return
	}
	item := items[0]
	items[0] = nil
	w.closeTabThen(item, func(closed bool) {
		if !closed {
			w.closingDocuments = false
			return
		}
		w.closeNextDocument(items[1:])
	})
}

func completeTabClose(done func(bool), closed bool) {
	if done != nil {
		done(closed)
	}
}

func (w *Window) showDocumentTabMenu(item *container.TabItem, event *fyne.PointEvent) {
	if w.Window != nil {
		showContextMenu(w.documentTabMenu(item), w.Window.Canvas(), event.AbsolutePosition)
	}
}
