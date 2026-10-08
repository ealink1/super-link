package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/layout"
)

// toolbarLayout preserves SuperLink's compact bar height while retaining HBox sizing.
type toolbarLayout struct{ height float32 }

func (l *toolbarLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	size := layout.NewHBoxLayout().MinSize(objects)
	size.Height = l.height
	return size
}
func (l *toolbarLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	layout.NewHBoxLayout().Layout(objects, size)
}
