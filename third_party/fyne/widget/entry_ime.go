package widget

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/internal/cache"
)

// InputMethodCaret returns the actual rendered caret, including its containing
// scroll hierarchy. Native text-input integrations can use the driver's
// AbsolutePositionForObject to anchor candidates without accessing text.
func (e *Entry) InputMethodCaret() (fyne.CanvasObject, fyne.Position, fyne.Size) {
	if e.content == nil {
		return nil, fyne.Position{}, fyne.Size{}
	}
	renderer, ok := cache.Renderer(e.content).(*entryContentRenderer)
	if !ok {
		return nil, fyne.Position{}, fyne.Size{}
	}
	return e.content, renderer.cursor.Position(), renderer.cursor.Size()
}
