package ui

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/infra/drivers"
	"github.com/ealink1/super-link/internal/infra/release"
	"github.com/ealink1/super-link/internal/infra/secrets"
	"github.com/ealink1/super-link/internal/infra/state"
)

func shellTestWindow(t *testing.T) *Window {
	t.Helper()
	app := test.NewApp()
	t.Cleanup(app.Quit)
	root := t.TempDir()
	store, err := state.Open(filepath.Join(root, "state.sqlite"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	profiles := &application.Profiles{Store: store, Vault: secrets.New()}
	engine := application.NewEngine(profiles)
	t.Cleanup(func() { engine.Close() })
	w := newTestWindow(t, app, Dependencies{Profiles: profiles, Engine: engine, Drivers: &drivers.Manager{Root: root}, Releases: release.New(""), Root: root, Version: "test", Close: func() error { return nil }})
	w.Window.Show()
	waitReady(t, w)
	t.Cleanup(func() { w.switcher.stop(); w.jobs.stop(); w.Window.SetCloseIntercept(nil); w.Window.Close() })
	return w
}
func pumpShell(t *testing.T, w *Window, condition func() bool) {
	t.Helper()
	value, _ := testUIQueues.Load(w)
	queue := value.(chan func())
	timer := time.NewTimer(8 * time.Second)
	defer timer.Stop()
	for !condition() {
		select {
		case callback := <-queue:
			callback()
		case <-time.After(time.Millisecond):
		case <-timer.C:
			t.Fatal("Shell UI completion timeout")
		}
	}
}
func TestSQLShellSwitchPreservesSeparateState(t *testing.T) {
	w := shellTestWindow(t)
	sql := w.switcher.sql
	selected := w.tabs.Selected()
	if w.switcher.shell != nil {
		t.Fatal("Shell eagerly loaded")
	}
	w.switcher.selectMode(1)
	shell := w.switcher.shell
	waitUI(t, w)
	shell.showHosts()
	search := shell.search
	search.SetText("kept")
	for range 3 {
		w.switcher.selectMode(0)
		if w.switcher.body.Objects[0] != sql || w.tabs.Selected() != selected {
			t.Fatal("SQL state replaced")
		}
		w.switcher.selectMode(1)
		if w.switcher.shell != shell || shell.search != search || search.Text != "kept" || shell.page != 0 {
			t.Fatal("Shell state replaced")
		}
	}
	if len(w.tabs.Items) != 0 || len(w.workspaces) != 0 || len(shell.tabs.Items) != 0 {
		t.Fatal("switching started sessions or SQL")
	}
}

func TestTerminalANSIUnicodeInputAndBoundedScrollback(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	surface := newTerminalSurface(nil)
	defer surface.dispose()
	input := make(chan string, 4)
	done := make(chan struct{})
	go func() {
		defer close(done)
		buffer := make([]byte, 128)
		for {
			n, err := surface.emulator.Read(buffer)
			if n > 0 {
				input <- string(buffer[:n])
			}
			if err != nil {
				return
			}
		}
	}()
	w := app.NewWindow("terminal")
	w.SetContent(surface)
	w.Resize(fyne.NewSize(480, 240))
	w.Show()
	defer w.Close()
	surface.feed([]byte("\x1b[31;44m中文\x1b[0mabc"))
	first := surface.emulator.CellAt(0, 0)
	if first == nil || first.Content != "中" || first.Width != 2 || surface.emulator.CellAt(4, 0).Content != "a" {
		t.Fatal("CJK cell positions lost")
	}
	renderer := test.WidgetRenderer(surface).(*terminalRenderer)
	if renderer.pool[0].Color == terminalForeground || renderer.backgrounds[0].FillColor == terminalBackground {
		t.Fatal("ANSI foreground/background lost")
	}
	surface.feed([]byte("\x1b[?25l"))
	if surface.visibleCursor {
		t.Fatal("cursor visibility ignored")
	}
	surface.TypedRune('好')
	select {
	case raw := <-input:
		if raw != "好" {
			t.Fatal(raw)
		}
	case <-time.After(time.Second):
		t.Fatal("typing blocked")
	}
	surface.TypedShortcut(&desktop.CustomShortcut{KeyName: fyne.KeyC, Modifier: fyne.KeyModifierControl})
	select {
	case raw := <-input:
		if raw != "\x03" {
			t.Fatal("Ctrl-C did not interrupt", raw)
		}
	case <-time.After(time.Second):
		t.Fatal("Ctrl-C blocked")
	}
	surface.TypedKey(&fyne.KeyEvent{Name: fyne.KeyUp})
	select {
	case raw := <-input:
		if raw != "\x1b[A" {
			t.Fatal("arrow encoding wrong", raw)
		}
	case <-time.After(time.Second):
		t.Fatal("arrow input blocked")
	}
	surface.feed([]byte(strings.Repeat("line\r\n", 1100)))
	if surface.emulator.ScrollbackLen() != 1000 {
		t.Fatal("scrollback budget not enforced")
	}
	surface.Scrolled(&fyne.ScrollEvent{Scrolled: fyne.Delta{DY: 10}})
	if surface.scroll == 0 {
		t.Fatal("scrollback inaccessible")
	}
	surface.feed([]byte("\x1b[?1049h"))
	if !surface.altScreen || surface.scroll != 0 {
		t.Fatal("alternate screen reused scrollback")
	}
	surface.dispose()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("input reader leaked")
	}
}

func TestShellLocalSessionSurvivesSwitchAndCloses(t *testing.T) {
	if runtime.GOOS != "darwin" && runtime.GOOS != "linux" {
		t.Skip("local PTY is not implemented on this platform")
	}
	t.Setenv("HOME", t.TempDir())
	t.Setenv("SHELL", "/bin/sh")
	t.Setenv("ENV", "")
	fixture := filepath.Join(t.TempDir(), "fixture-shell")
	if err := os.WriteFile(fixture, []byte("#!/bin/sh\nexec /bin/sh -i\n"), 0700); err != nil {
		t.Fatal(err)
	}
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	s.openLocal(fixture)
	p := s.panes[s.tabs.Selected()]
	pumpShell(t, w, func() bool { return p.session != nil || p.ended })
	if p.ended {
		t.Fatal(p.status.Text)
	}
	p.terminal.emulator.SendText("printf 'NAVI_%s\\n' 'OK'\n")
	pumpShell(t, w, func() bool { return strings.Contains(p.terminal.emulator.String(), "NAVI_OK") })
	w.switcher.selectMode(0)
	w.switcher.selectMode(1)
	if s.panes[p.item] != p || p.closed || p.ended {
		t.Fatal("switch killed session")
	}
	s.tabs.CloseIntercept(p.item)
	pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
	if !p.closed || len(s.panes) != 0 {
		t.Fatal("session not released")
	}
	if _, err := w.Store.Profiles(context.Background()); err != nil {
		t.Fatal("Shell close affected SQL store", err)
	}
}

func TestShellClosingDuringStartupJoinsAllWorkers(t *testing.T) {
	if runtime.GOOS != "darwin" && runtime.GOOS != "linux" {
		t.Skip("local PTY is not implemented")
	}
	t.Setenv("HOME", t.TempDir())
	t.Setenv("ENV", "")
	fixture := filepath.Join(t.TempDir(), "fixture-shell")
	if err := os.WriteFile(fixture, []byte("#!/bin/sh\nexec /bin/sh -i\n"), 0700); err != nil {
		t.Fatal(err)
	}
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	for range 5 {
		s.openLocal(fixture)
		s.tabs.CloseIntercept(s.tabs.Selected())
	}
	pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
	if len(s.panes) != 0 {
		t.Fatal("startup close retained tabs")
	}
}
