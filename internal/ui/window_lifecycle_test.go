package ui

import (
	"context"
	"runtime"
	"sync/atomic"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"github.com/ealink1/super-link/internal/domain"
)

type lifecycleWindow struct {
	fyne.Window
	hides, shows, focuses int
}

func (w *lifecycleWindow) Hide()         { w.hides++; w.Window.Hide() }
func (w *lifecycleWindow) Show()         { w.shows++; w.Window.Show() }
func (w *lifecycleWindow) RequestFocus() { w.focuses++; w.Window.RequestFocus() }

type lifecycleApp struct {
	fyne.App
	quits int
}

func (a *lifecycleApp) Quit() { a.quits++ }

func TestWindowHideRestorePreservesSessionsAndBackgroundWork(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	pane := w.switcher.shell.newPane("background")
	window := &lifecycleWindow{Window: w.Window}
	w.Window = window
	started, finish := make(chan struct{}), make(chan struct{})
	defer func() {
		select {
		case <-finish:
		default:
			close(finish)
		}
	}()
	completed := false
	w.jobs.run(func(ctx context.Context) (any, error) {
		close(started)
		select {
		case <-finish:
			return nil, nil
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}, func(any, error) { completed = true })
	<-started
	if runtime.GOOS == "darwin" {
		w.closeWindow()
	} else {
		w.hideWindow()
	}
	if window.hides != 1 || w.shuttingDown || w.jobs.closing.Load() || w.jobs.ctx.Err() != nil || pane.closed || pane.ctx.Err() != nil {
		t.Fatal("hiding cancelled work or released a session")
	}
	pane.terminal.feed([]byte("output while hidden\r\n"))
	close(finish)
	pumpShell(t, w, func() bool { return completed })
	content, selected := window.Content(), w.switcher.shell.tabs.Selected()
	w.restoreWindow()
	if window.shows != 1 || window.focuses != 1 || window.Content() != content || w.switcher.shell.tabs.Selected() != selected || !completed {
		t.Fatal("restoring rebuilt the workspace or interrupted the background job")
	}
	w.shuttingDown = true
	w.hideWindow()
	w.restoreWindow()
	if window.hides != 1 || window.shows != 1 {
		t.Fatal("lifecycle actions changed a shutting-down window")
	}
	w.shuttingDown = false
}

func TestQuitFromHiddenWindowSavesNotesAndReleasesSessions(t *testing.T) {
	w, note := noteTestWindow(t)
	note.newNote()
	note.editor.SetText("last edit before explicit quit")
	w.switcher.selectMode(1)
	waitUI(t, w)
	pane := w.switcher.shell.newPane("quit")
	window := &lifecycleWindow{Window: w.Window}
	w.Window = window
	app := &lifecycleApp{App: w.App}
	w.App = app
	var closes atomic.Int32
	w.onClose = func() error { closes.Add(1); return nil }
	w.hideWindow()
	action := w.requestQuit
	if menu := window.MainMenu(); menu != nil {
		for _, section := range menu.Items {
			for _, item := range section.Items {
				if item.IsQuit {
					action = item.Action
				}
			}
		}
	}
	action()
	pumpShell(t, w, func() bool { return app.quits == 1 })
	if window.shows != 1 || closes.Load() != 1 || !pane.closed || pane.ctx.Err() == nil || !w.jobs.closing.Load() {
		t.Fatal("explicit quit did not show the window and release background resources")
	}
	book, err := note.service.Load(context.Background())
	if err != nil || len(book.Notes) != 1 || book.Notes[0].Body != "last edit before explicit quit" {
		t.Fatal("explicit quit lost the final note edit", err)
	}
	w.requestQuit()
	if app.quits != 1 || closes.Load() != 1 {
		t.Fatal("quit cleanup ran twice")
	}
}

func TestQuitFromHiddenWindowKeepsUnsavedConfirmationVisible(t *testing.T) {
	w := shellTestWindow(t)
	window := &lifecycleWindow{Window: w.Window}
	w.Window = window
	item := container.NewTabItem("unsaved", nil)
	w.tables[item] = &tableWorkspace{edits: map[int]domain.RowChange{0: {}}}
	w.hideWindow()
	w.requestQuit()
	if window.shows != 1 || window.focuses != 1 || w.shuttingDown || window.Canvas().Overlays().Top() == nil {
		t.Fatal("hidden quit bypassed or hid the unsaved-edit confirmation")
	}
	delete(w.tables, item)
}
