package ui

import (
	"sync"
	"sync/atomic"
	"time"

	"fyne.io/fyne/v2"
)

type inputMethodCaret interface {
	InputMethodCaret() (fyne.CanvasObject, fyne.Position, fyne.Size)
}

// Track the focused control rather than maintaining a separate integration in
// every form. Only geometry is read; password and other input values stay private.
func startInputMethods(app fyne.App, dispatch func(func())) func() {
	if !nativeInputMethodsEnabled() {
		return func() {}
	}
	stop := make(chan struct{})
	var closed atomic.Bool
	var pending atomic.Bool
	var once sync.Once
	go func() {
		ticker := time.NewTicker(50 * time.Millisecond)
		defer ticker.Stop()
		for {
			select {
			case <-stop:
				return
			case <-ticker.C:
				if !pending.CompareAndSwap(false, true) {
					continue
				}
				dispatch(func() {
					defer pending.Store(false)
					if closed.Load() {
						return
					}
					for _, window := range app.Driver().AllWindows() {
						c := window.Canvas()
						provider, ok := c.Focused().(inputMethodCaret)
						var caret fyne.CanvasObject
						var offset fyne.Position
						var size fyne.Size
						if ok {
							caret, offset, size = provider.InputMethodCaret()
						}
						if caret == nil {
							publishInputMethod(window, fyne.Position{}, fyne.Size{}, false)
							continue
						}
						publishInputMethod(window, app.Driver().AbsolutePositionForObject(caret).Add(offset), size, true)
					}
				})
			}
		}
	}()
	return func() { once.Do(func() { closed.Store(true); close(stop) }) }
}
func (e *noteEntry) InputMethodCaret() (fyne.CanvasObject, fyne.Position, fyne.Size) {
	if !e.rich || e.richRenderer == nil {
		return e.Entry.InputMethodCaret()
	}
	point := e.richRenderer.nearestOffset(e.sourceOffset())
	return e, point.position, fyne.NewSize(2, point.height)
}
func (t *terminalSurface) InputMethodCaret() (fyne.CanvasObject, fyne.Position, fyne.Size) {
	if t.imeCaret == nil {
		return nil, fyne.Position{}, fyne.Size{}
	}
	return t, t.imeCaret.Position(), t.imeCaret.Size()
}
