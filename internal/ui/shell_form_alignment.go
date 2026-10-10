package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// Retain native Fyne input, selection, focus and password handling while
// aligning the single-line viewport with the compact form's optical center.
type shellFormEntry struct{ widget.Entry }

func newShellFormEntry(password bool) *shellFormEntry {
	e := &shellFormEntry{}
	e.Password, e.Wrapping = password, fyne.TextWrap(fyne.TextTruncateClip)
	e.ExtendBaseWidget(e)
	return e
}
func (e *shellFormEntry) CreateRenderer() fyne.WidgetRenderer {
	return &shellFormRenderer{content: e.Entry.CreateRenderer(), entry: &e.Entry, owner: e}
}

type shellFormSelect struct{ widget.Select }

func newShellFormSelect(options []string) *shellFormSelect {
	s := &shellFormSelect{}
	s.Options = options
	s.ExtendBaseWidget(s)
	return s
}
func (s *shellFormSelect) CreateRenderer() fyne.WidgetRenderer {
	return &shellFormRenderer{content: s.Select.CreateRenderer(), owner: s}
}

type shellFormCheck struct{ widget.Check }

func newShellFormCheck(text string) *shellFormCheck {
	c := &shellFormCheck{}
	c.Text = text
	c.ExtendBaseWidget(c)
	return c
}
func (c *shellFormCheck) CreateRenderer() fyne.WidgetRenderer {
	return &shellFormRenderer{content: c.Check.CreateRenderer(), owner: c}
}

type shellFormRenderer struct {
	content fyne.WidgetRenderer
	entry   *widget.Entry
	owner   fyne.CanvasObject
}

func (r *shellFormRenderer) MinSize() fyne.Size           { return r.content.MinSize() }
func (r *shellFormRenderer) Objects() []fyne.CanvasObject { return r.content.Objects() }
func (r *shellFormRenderer) Destroy()                     { r.content.Destroy() }
func (r *shellFormRenderer) Refresh()                     { r.content.Refresh(); r.Layout(r.owner.Size()) }
func (r *shellFormRenderer) Layout(size fyne.Size) {
	r.content.Layout(size)
	for _, object := range r.content.Objects() {
		if r.entry != nil {
			scroll, ok := object.(*container.Scroll)
			if !ok {
				continue
			}
			height := scroll.Content.MinSize().Height
			height = min(height, size.Height)
			scroll.Resize(fyne.NewSize(scroll.Size().Width, height))
			scroll.Move(fyne.NewPos(scroll.Position().X, (size.Height-height)/2-2))
			continue
		}
		switch object.(type) {
		case *widget.RichText:
			height := object.MinSize().Height
			object.Resize(fyne.NewSize(object.Size().Width, height))
			object.Move(fyne.NewPos(object.Position().X, (size.Height-height)/2))
		case *canvas.Text:
			height := object.MinSize().Height
			object.Resize(fyne.NewSize(object.Size().Width, height))
			object.Move(fyne.NewPos(object.Position().X, (size.Height-height)/2-2))
		}
	}
}
