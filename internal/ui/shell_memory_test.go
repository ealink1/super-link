package ui

import (
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestShellStripReusesPagesAndDetachesClosedTabs(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	first := container.NewTabItem("first", widget.NewLabel("one"))
	second := container.NewTabItem("second", widget.NewLabel("two"))
	s.tabs.Append(first)
	s.tabs.Append(second)
	s.showTerminals()
	page, strip, body := s.terminalPage, s.terminalStrip, s.terminalBody
	a, b := s.terminalButtons[first], s.terminalButtons[second]
	for i := range 100 {
		s.tabs.Select(s.tabs.Items[i%2])
		s.showTerminals()
	}
	if page != s.terminalPage || strip != s.terminalStrip || body != s.terminalBody || a != s.terminalButtons[first] || b != s.terminalButtons[second] {
		t.Fatal("switching recreated the terminal page or tab controls")
	}
	first.Text = "renamed"
	s.showTerminals()
	test.Tap(a)
	if s.tabs.Selected() != first || a.title != "renamed" || !a.selected || b.selected || body.Objects[0] != first.Content {
		t.Fatal("selection or renaming lost")
	}
	s.tabs.CloseIntercept(first)
	if a.selectTab != nil || a.closeTab != nil || s.terminalButtons[first] != nil || body.Objects[0] != second.Content {
		t.Fatal("removed tab retained callbacks or content")
	}
	test.Tap(a)
	if s.tabs.Selected() != second {
		t.Fatal("stale header reopened a closed tab")
	}
	s.tabs.CloseIntercept(second)
	s.showTerminals()
	if len(strip.Objects) != 0 || body.Objects[0] != s.terminalEmpty {
		t.Fatal("last closed page still retained")
	}
	welcome := s.terminalEmpty
	for range 10 {
		s.showTerminals()
	}
	if s.terminalEmpty != welcome {
		t.Fatal("empty workspace recreated welcome controls")
	}
}

func TestClosedShellTabDropsEmulatorAndStopsPipeWorker(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	p := s.newPane("fixture")
	p.terminal.feed([]byte("finished output\r\n"))
	renderer := test.WidgetRenderer(p.terminal).(*terminalRenderer)
	p.finish(nil)
	if p.terminal.emulator == nil || p.terminal.emulator.String() == "" {
		t.Fatal("ended session lost its last screen")
	}
	s.tabs.CloseIntercept(p.item)
	pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
	if p.terminal.emulator != nil || len(renderer.pool) != 0 || len(renderer.text.Objects) != 0 || len(s.panes) != 0 {
		t.Fatal("closed terminal retained its screen, parser or renderer objects")
	}
	// Cached focus/layout events arriving after removal must stay harmless.
	p.terminal.FocusGained()
	p.terminal.FocusLost()
	p.terminal.Resize(fyne.NewSize(640, 480))
	p.terminal.TypedRune('a')
	p.terminal.Scrolled(&fyne.ScrollEvent{Scrolled: fyne.Delta{DY: 1}})
	p.stop()
}

func TestHiddenTerminalUpdatesScreenWithoutBuildingBlankTextures(t *testing.T) {
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	terminal := newTerminalSurface(nil)
	defer terminal.release()
	renderer := test.WidgetRenderer(terminal).(*terminalRenderer)
	if len(renderer.pool) != 0 || len(renderer.text.Objects) != 0 {
		t.Fatal("empty terminal allocated full-width blank text rows")
	}
	terminal.active = func() bool { return false }
	terminal.feed([]byte("中文 hidden"))
	if terminal.emulator.CellAt(0, 0).Content != "中" || len(renderer.pool) != 0 {
		t.Fatal("hidden output was lost or created text textures")
	}
	terminal.active = func() bool { return true }
	terminal.Refresh()
	if len(renderer.pool) != 1 || renderer.pool[0].Text != "中文 hidden" {
		t.Fatal("restoring terminal did not render the latest screen")
	}
	// Spaces with an explicit background remain visible (e.g. Vim status bars).
	terminal.feed([]byte("\r\n\x1b[44m   \x1b[0m"))
	if len(renderer.pool) < 2 || renderer.pool[1].Text != "   " {
		t.Fatal("styled blank cells were trimmed")
	}
}
