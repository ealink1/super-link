package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
)

func newShellTab(s *shellWorkspace, item *container.TabItem) *documentTab {
	tab := newDocumentTab(item.Text, "", s.tabs.Selected() == item, func() {
		s.tabs.Select(item)
		s.showTerminals()
		if pane := s.panes[item]; pane != nil && !pane.ended {
			s.owner.Window.Canvas().Focus(pane.terminal)
		}
	}, func() {
		s.tabs.CloseIntercept(item)
		s.showTerminals()
	})
	tab.showMenu = func(event *fyne.PointEvent) {
		showContextMenu(s.terminalTabMenu(item), s.owner.Window.Canvas(), event.AbsolutePosition)
	}
	return tab
}

// The visible strip is the only owner of terminal page layout. Reuse its
// controls and detach closed callbacks while cached renderers expire.
func (s *shellWorkspace) syncTerminalTabs() {
	if s.terminalStrip == nil {
		return
	}
	selected := s.tabs.Selected()
	buttons := make([]fyne.CanvasObject, 0, len(s.tabs.Items))
	changedStrip := len(s.terminalStrip.Objects) != len(s.tabs.Items)
	active := make(map[*container.TabItem]bool, len(s.tabs.Items))
	for _, item := range s.tabs.Items {
		active[item] = true
		button := s.terminalButtons[item]
		if button == nil {
			button = newShellTab(s, item)
			s.terminalButtons[item] = button
		}
		if button.title != item.Text || button.selected != (item == selected) {
			button.title, button.selected = item.Text, item == selected
			button.Refresh()
		}
		if len(buttons) >= len(s.terminalStrip.Objects) || s.terminalStrip.Objects[len(buttons)] != button {
			changedStrip = true
		}
		buttons = append(buttons, button)
	}
	for item, button := range s.terminalButtons {
		if !active[item] {
			button.selectTab, button.closeTab = nil, nil
			button.showMenu = nil
			delete(s.terminalButtons, item)
		}
	}
	if changedStrip {
		s.terminalStrip.Objects = buttons
		s.terminalStrip.Refresh()
	}
	var content fyne.CanvasObject
	if selected != nil {
		content = selected.Content
	} else {
		if s.terminalEmpty == nil {
			s.terminalEmpty = container.New(&shellWelcomeLayout{}, s.terminalWelcome())
		}
		content = s.terminalEmpty
	}
	if len(s.terminalBody.Objects) != 1 || s.terminalBody.Objects[0] != content {
		s.terminalBody.Objects = []fyne.CanvasObject{content}
		s.terminalBody.Refresh()
		if pane := s.panes[selected]; pane != nil {
			pane.terminal.Refresh()
		}
	}
}

func truncateShellTitle(text string, limit int) string {
	runes := []rune(text)
	if len(runes) > limit {
		return string(runes[:limit-1]) + "…"
	}
	return text
}
