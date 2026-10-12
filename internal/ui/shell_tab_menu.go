package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
)

func (s *shellWorkspace) terminalTabMenu(anchor *container.TabItem) *fyne.Menu {
	return s.tabs.closeMenu(anchor, func(group tabCloseGroup) { s.closeTerminalTabs(anchor, group) })
}

func (s *shellWorkspace) closeTerminalTabs(anchor *container.TabItem, group tabCloseGroup) {
	// Remove mutates the backing array, so snapshot targets before closing any.
	items := s.tabs.tabsToClose(anchor, group)
	if len(items) == 0 {
		return
	}
	for _, item := range items {
		s.tabs.CloseIntercept(item)
	}
	s.showTerminals()
	if pane := s.panes[s.tabs.Selected()]; pane != nil && !pane.ended {
		s.owner.Window.Canvas().Focus(pane.terminal)
	}
}
