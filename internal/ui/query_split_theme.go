package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
)

// Retain Fyne's split dragging and pane constraints, but theme only its
// divider: the eight-pixel hit area must not become an eight-pixel stripe.
type querySplit struct{ container.Split }

func newQuerySplit(top, bottom fyne.CanvasObject) *querySplit {
	s := &querySplit{Split: container.Split{Leading: top, Trailing: bottom, Offset: 0.53}}
	s.BaseWidget.ExtendBaseWidget(s)
	return s
}

func newQueryNavigationSplit(left, right fyne.CanvasObject) *querySplit {
	s := newQuerySplit(left, right)
	s.Horizontal = true
	s.Offset = 0.18
	return s
}

func (s *querySplit) CreateRenderer() fyne.WidgetRenderer {
	base := s.Split.CreateRenderer()
	scope := container.NewThemeOverride(base.Objects()[1], queryDividerTheme{})
	return &querySplitRenderer{WidgetRenderer: base, split: s, scope: scope, scopeRenderer: scope.CreateRenderer(), line: canvas.NewRectangle(queryDividerLineColor())}
}

type querySplitRenderer struct {
	fyne.WidgetRenderer
	split         *querySplit
	scope         *container.ThemeOverride
	scopeRenderer fyne.WidgetRenderer
	line          *canvas.Rectangle
}

func (r *querySplitRenderer) Objects() []fyne.CanvasObject {
	return append(append([]fyne.CanvasObject{}, r.WidgetRenderer.Objects()...), r.line)
}

func (r *querySplitRenderer) Layout(size fyne.Size) {
	r.WidgetRenderer.Layout(size)
	divider := r.scope.Content
	if r.split.Horizontal {
		r.line.Move(divider.Position().Add(fyne.NewPos((divider.Size().Width-1)/2, 0)))
		r.line.Resize(fyne.NewSize(1, size.Height))
	} else {
		r.line.Move(divider.Position().Add(fyne.NewPos(0, (divider.Size().Height-1)/2)))
		r.line.Resize(fyne.NewSize(size.Width, 1))
	}
	if divider.Visible() {
		r.line.Show()
	} else {
		r.line.Hide()
	}
}

func (r *querySplitRenderer) Refresh() {
	r.WidgetRenderer.Refresh()
	r.scope.Refresh()
	r.line.FillColor = queryDividerLineColor()
	r.Layout(r.split.Size())
	r.line.Refresh()
}

func (r *querySplitRenderer) Destroy() {
	r.scopeRenderer.Destroy()
	r.WidgetRenderer.Destroy()
}

func queryDividerLineColor() color.Color {
	shade := color.NRGBAModel.Convert(theme.Color(theme.ColorNameForeground)).(color.NRGBA)
	shade.A = 64
	return shade
}

type queryDividerTheme struct{ querySplitTheme }

func (queryDividerTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameShadow || name == theme.ColorNameHover {
		return color.Transparent
	}
	return fyne.CurrentApp().Settings().Theme().Color(name, variant)
}

// Keep the query divider easy to grab without changing surrounding controls.
type querySplitTheme struct{}

func (querySplitTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameSplitThickness {
		return 8
	}
	return fyne.CurrentApp().Settings().Theme().Size(name)
}

func (querySplitTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	return fyne.CurrentApp().Settings().Theme().Color(name, variant)
}

func (querySplitTheme) Font(style fyne.TextStyle) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Font(style)
}

func (querySplitTheme) Icon(name fyne.ThemeIconName) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Icon(name)
}
