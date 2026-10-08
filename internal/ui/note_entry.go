package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
)

// noteEntry keeps Fyne's text editing, focus and cursor animation. InputBorder
// must stay positive because Fyne also uses it as the caret's width. Remove
// only the outer outline from the renderer instead of zeroing that theme size.
type noteEntry struct {
	widget.Entry
	rich            bool
	richFocused     bool
	selectionAnchor int
	richShift       bool
	richRenderer    *noteRichRenderer
	history         noteEditHistory
}

func newNoteEntry(multiline bool) *noteEntry {
	e := &noteEntry{}
	e.MultiLine = multiline
	e.Wrapping = fyne.TextWrap(fyne.TextTruncateClip)
	if multiline {
		e.Wrapping = fyne.TextWrapWord
	}
	e.ExtendBaseWidget(e)
	return e
}

func (e *noteEntry) CreateRenderer() fyne.WidgetRenderer {
	renderer := e.Entry.CreateRenderer()
	// The Entry's outer objects contain its input outline; the blinking cursor
	// lives inside the nested text content. No private Fyne fields are accessed.
	for _, object := range renderer.Objects() {
		if rectangle, ok := object.(*canvas.Rectangle); ok && rectangle.StrokeWidth > 0 {
			rectangle.Hide()
		}
	}
	if e.rich {
		e.richRenderer = &noteRichRenderer{entry: e, base: renderer, width: 600}
		e.richRenderer.draw()
		return e.richRenderer
	}
	return renderer
}

// Preserve the extended renderer: Entry.MinSize re-extends itself as Entry.
func (e *noteEntry) MinSize() fyne.Size { return e.BaseWidget.MinSize() }
