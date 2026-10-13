package ui

import "runtime"

func (w *Window) closeWindow() {
	if runtime.GOOS == "darwin" {
		w.hideWindow()
		return
	}
	w.shutdown()
}

func (w *Window) hideWindow() {
	if !w.shuttingDown {
		w.Window.Hide()
	}
}

func (w *Window) restoreWindow() {
	if w.shuttingDown {
		return
	}
	w.Window.Show()
	w.Window.RequestFocus()
}

func (w *Window) requestQuit() {
	// Unsaved-edit confirmations must remain reachable when quitting from the
	// application menu or Dock while the main window is hidden.
	w.restoreWindow()
	w.shutdown()
}
