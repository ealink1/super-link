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
	// Keep the theme scope in the painted tree so its renderer cannot expire
	// while the divider remains visible. The split owns the divider's geometry.
	scope.CreateRenderer().Objects()[0].(*fyne.Container).Layout = queryDividerScopeLayout{}
	line := canvas.NewRectangle(queryDividerLineColor())
	r := &querySplitRenderer{WidgetRenderer: base, split: s, scope: scope, line: line,
		objects: []fyne.CanvasObject{s.Leading, scope, s.Trailing, line}}
	// Renderer recreation can happen without a subsequent widget resize.
	r.Layout(s.Size())
	return r
}

type querySplitRenderer struct {
	fyne.WidgetRenderer
	split   *querySplit
	scope   *container.ThemeOverride
	line    *canvas.Rectangle
	objects []fyne.CanvasObject
}

func (r *querySplitRenderer) Objects() []fyne.CanvasObject {
	return r.objects
}

func (r *querySplitRenderer) Layout(size fyne.Size) {
	r.WidgetRenderer.Layout(size)
	r.scope.Move(fyne.Position{})
	r.scope.Resize(size)
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
	r.objects[0], r.objects[2] = r.split.Leading, r.split.Trailing
	r.scope.Refresh()
	r.line.FillColor = queryDividerLineColor()
	r.Layout(r.split.Size())
	r.line.Refresh()
}

type queryDividerScopeLayout struct{}

func (queryDividerScopeLayout) Layout([]fyne.CanvasObject, fyne.Size) {}

func (queryDividerScopeLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[0].MinSize()
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
