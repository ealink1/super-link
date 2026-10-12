package ui

import (
	"slices"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestShellTabMenuClosesGroupsAndReleasesSessions(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	for _, tc := range []struct {
		name      string
		group     tabCloseGroup
		remaining []int
	}{
		{"left", tabsLeft, []int{2, 3, 4}},
		{"right", tabsRight, []int{0, 1, 2}},
		{"others", tabsOthers, []int{2}},
		{"all", tabsAll, nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			panes := make([]*shellPane, 5)
			for i := range panes {
				panes[i] = s.newPane(string(rune('a' + i)))
				panes[i].terminal.feed([]byte("session output\r\n"))
			}
			headers := make([]*documentTab, len(panes))
			for i, p := range panes {
				headers[i] = s.terminalButtons[p.item]
			}
			anchor := panes[2].item
			header := s.terminalButtons[anchor]
			position := w.App.Driver().AbsolutePositionForObject(header).Add(fyne.NewPos(12, 16))
			test.TapCanvas(w.Window.Canvas(), position)
			popup, ok := w.Window.Canvas().Focused().(*widget.PopUpMenu)
			if !ok || len(popup.Items) != 4 || w.Window.Canvas().Overlays().Top() == nil || s.tabs.Selected() != anchor {
				t.Fatal("click did not select the tab and show four menu actions")
			}
			test.Tap(popup.Items[tc.group].(fyne.Tappable))
			if w.Window.Canvas().Overlays().Top() != nil {
				t.Fatal("menu remained visible after an action")
			}
			if len(s.tabs.Items) != len(tc.remaining) || len(s.panes) != len(tc.remaining) {
				t.Fatal("wrong number of sessions closed")
			}
			for i, p := range panes {
				retained := slices.Contains(tc.remaining, i)
				if p.closed == retained || (p.ctx.Err() != nil) == retained || (p.terminal.emulator != nil) != retained {
					t.Fatalf("session %d was not retained or fully released", i)
				}
				if !retained && (s.terminalButtons[p.item] != nil || headers[i].showMenu != nil || headers[i].selectTab != nil || headers[i].closeTab != nil) {
					t.Fatal("removed tab retained its header or callbacks")
				}
			}
			for i, original := range tc.remaining {
				if s.tabs.Items[i] != panes[original].item {
					t.Fatal("surviving tabs were reordered")
				}
			}
			if tc.group == tabsAll {
				if s.tabs.Selected() != nil || header.showMenu != nil || header.selectTab != nil || header.closeTab != nil || s.terminalBody.Objects[0] != s.terminalEmpty {
					t.Fatal("closing all did not clear callbacks, selection and content")
				}
			} else {
				if s.tabs.Selected() != anchor || s.terminalBody.Objects[0] != anchor.Content || w.Window.Canvas().Focused() != panes[2].terminal {
					t.Fatal("surviving anchor lost selection, content or terminal focus")
				}
				s.closeTerminalTabs(anchor, tabsAll)
			}
			pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
		})
	}
}

func TestShellTabMenuDisablesEmptyGroupsAndIgnoresRemovedAnchor(t *testing.T) {
	a, b, c := container.NewTabItem("a", nil), container.NewTabItem("b", nil), container.NewTabItem("c", nil)
	s := &shellWorkspace{tabs: &documents{Items: []*container.TabItem{a, b, c}}}
	for _, tc := range []struct {
		anchor   *container.TabItem
		disabled []bool
	}{
		{a, []bool{true, false, false, false}},
		{b, []bool{false, false, false, false}},
		{c, []bool{false, true, false, false}},
	} {
		menu := s.terminalTabMenu(tc.anchor)
		for i, item := range menu.Items {
			if item.Disabled != tc.disabled[i] {
				t.Fatalf("%s on %s has wrong enabled state", item.Label, tc.anchor.Text)
			}
		}
	}
	stale := s.terminalTabMenu(b)
	s.tabs.Remove(b)
	closed := 0
	s.tabs.CloseIntercept = func(*container.TabItem) { closed++ }
	for _, item := range stale.Items {
		item.Action()
	}
	if closed != 0 || !slices.Equal(s.tabs.Items, []*container.TabItem{a, c}) {
		t.Fatal("stale menu closed surviving sessions")
	}
	s.tabs.Items = []*container.TabItem{a}
	menu := s.terminalTabMenu(a)
	for i, item := range menu.Items {
		if item.Disabled != (i != int(tabsAll)) {
			t.Fatal("single tab should only allow closing all")
		}
	}
}

func TestShellTabRightClickAndCloseButton(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	first, second := s.newPane("first"), s.newPane("second")
	header := s.terminalButtons[first.item]
	test.TapSecondary(header)
	popup, ok := w.Window.Canvas().Focused().(*widget.PopUpMenu)
	if !ok || s.tabs.Selected() != first.item {
		t.Fatal("right click did not select and open the tab menu")
	}
	popup.TypedKey(&fyne.KeyEvent{Name: fyne.KeyEscape})
	test.Tap(test.WidgetRenderer(header).(*documentTabRenderer).close)
	if !first.closed || second.closed || s.tabs.Selected() != second.item || w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("close button did not close only its tab without opening a menu")
	}
	test.Tap(header)
	test.TapSecondary(header)
	if w.Window.Canvas().Overlays().Top() != nil {
		t.Fatal("removed tab reopened a menu")
	}
	s.closeTerminalTabs(second.item, tabsAll)
	pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
}
