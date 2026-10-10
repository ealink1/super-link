package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

// Center the controls, then compensate for the optical difference between
// numeric input text and the desktop font used by Chinese button captions.
type pagingRowLayout struct{}

func (pagingRowLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	size := layout.NewHBoxLayout().MinSize(objects)
	size.Height += 4 // Leave room for the optical offsets without clipping inputs.
	return size
}

func (pagingRowLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	layout.NewHBoxLayout().Layout(objects, size)
	for _, object := range objects {
		if !object.Visible() {
			continue
		}
		if _, spacer := object.(layout.SpacerObject); spacer {
			continue
		}
		height := object.MinSize().Height
		object.Resize(fyne.NewSize(object.Size().Width, height))
		offset := float32(0)
		switch object.(type) {
		case *widget.Entry, *widget.Select:
			offset = 2
		case *widget.Button:
			offset = -1
		}
		object.Move(fyne.NewPos(object.Position().X, (size.Height-height)/2+offset))
	}
}
