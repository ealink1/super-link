package ui

import (
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

// codeEntry keeps Fyne's text editing, IME, undo and selection implementation.
// A display-only token layer shares its scroll content and exact source text.
type codeEntry struct {
	widget.Entry
	layer    *syntaxLayer
	colors   *codeColorState
	onScroll func(fyne.Position)
	onRun    func()
}

func (e *codeEntry) TypedShortcut(shortcut fyne.Shortcut) {
	if key, ok := shortcut.(fyne.KeyboardShortcut); ok && key.Key() == fyne.KeyR && (key.Mod() == fyne.KeyModifierSuper || key.Mod() == fyne.KeyModifierControl || key.Mod() == fyne.KeyModifierShortcutDefault) && e.onRun != nil {
		e.onRun()
		return
	}
	e.Entry.TypedShortcut(shortcut)
}

func newCodeEntry() *codeEntry {
	e := &codeEntry{Entry: widget.Entry{MultiLine: true, Wrapping: fyne.TextWrapOff, TextStyle: fyne.TextStyle{Monospace: true}}, colors: &codeColorState{unwrapped: true}}
	e.ExtendBaseWidget(e)
	return e
}
func (e *codeEntry) CreateRenderer() fyne.WidgetRenderer {
	base := e.Entry.CreateRenderer()
	e.layer = &syntaxLayer{entry: e}
	e.layer.ExtendBaseWidget(e.layer)
	for _, object := range base.Objects() {
		if scroll, ok := object.(*container.Scroll); ok {
			original := scroll.Content
			scroll.Content = container.NewStack(e.layer, original)
			scrolled := scroll.OnScrolled
			scroll.OnScrolled = func(position fyne.Position) {
				if scrolled != nil {
					scrolled(position)
				}
				if e.onScroll != nil {
					e.onScroll(position)
				}
			}
		}
	}
	return &codeRenderer{WidgetRenderer: base, entry: e}
}

type codeRenderer struct {
	fyne.WidgetRenderer
	entry *codeEntry
}

func (r *codeRenderer) Layout(size fyne.Size) {
	r.WidgetRenderer.Layout(size)
	r.syncScroll()
}

// Cursor navigation and resizing also move Entry's scroll position, without
// emitting OnScrolled. Keep the line-number viewport aligned in those cases.
func (r *codeRenderer) syncScroll() {
	if r.entry.onScroll == nil {
		return
	}
	for _, object := range r.WidgetRenderer.Objects() {
		if scroll, ok := object.(*container.Scroll); ok {
			r.entry.onScroll(scroll.Offset)
			return
		}
	}
}

func (r *codeRenderer) Refresh() {
	r.entry.colors.unwrapped = r.entry.Wrapping == fyne.TextWrapOff
	r.WidgetRenderer.Refresh()
	objects := r.WidgetRenderer.Objects()
	if len(objects) > 1 {
		if border, ok := objects[1].(*canvas.Rectangle); ok {
			border.StrokeColor = color.Transparent
			border.Refresh()
		}
	}
	if r.entry.Wrapping == fyne.TextWrapOff {
		r.entry.layer.Show()
	} else {
		r.entry.layer.Hide()
	}
	r.entry.layer.Refresh()
	r.syncScroll()
}

// A cached theme needs only display state, not the editor and its callbacks.
type codeColorState struct{ unwrapped bool }
type codeTheme struct{ state *codeColorState }

func (t codeTheme) Font(style fyne.TextStyle) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Font(style)
}
func (t codeTheme) Icon(name fyne.ThemeIconName) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Icon(name)
}
func (t codeTheme) Size(name fyne.ThemeSizeName) float32 {
	return fyne.CurrentApp().Settings().Theme().Size(name)
}
func (t codeTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameInputBackground {
		return fyne.CurrentApp().Settings().Theme().Color(theme.ColorNameBackground, variant)
	}
	if name == theme.ColorNameForeground && t.state.unwrapped {
		return color.Transparent
	}
	if name == theme.ColorNamePrimary {
		return fyne.CurrentApp().Settings().Theme().Color(theme.ColorNameForeground, variant)
	}
	if name == theme.ColorNameInputBorder {
		return color.Transparent
	}
	return fyne.CurrentApp().Settings().Theme().Color(name, variant)
}

type syntaxLayer struct {
	widget.BaseWidget
	entry *codeEntry
}

func (s *syntaxLayer) CreateRenderer() fyne.WidgetRenderer {
	r := &syntaxRenderer{layer: s}
	r.update()
	return r
}

type syntaxRenderer struct {
	layer   *syntaxLayer
	source  string
	dark    bool
	pieces  []syntaxPiece
	objects []fyne.CanvasObject
	min     fyne.Size
}
type syntaxPiece struct {
	text *canvas.Text
	x, y float32
}

func (r *syntaxRenderer) update() {
	text := r.layer.entry.Text
	dark := false
	if t, ok := fyne.CurrentApp().Settings().Theme().(Theme); ok {
		dark = t.Dark
	}
	if r.source == text && r.dark == dark && r.objects != nil {
		return
	}
	r.source = text
	r.dark = dark
	r.pieces = nil
	r.objects = nil
	font := fyne.TextStyle{Monospace: true}
	size := theme.TextSize()
	letter := fyne.MeasureText("M", size, font)
	lineHeight := letter.Height
	x, y := float32(6), float32(4)
	maxWidth := float32(0)
	for _, token := range sqlworkbench.Lex(text) {
		parts := strings.Split(token.Text, "\n")
		for i, part := range parts {
			if i > 0 {
				maxWidth = max(maxWidth, x)
				x = 6
				y += lineHeight
			}
			if part == "" {
				continue
			}
			object := canvas.NewText(part, tokenColor(token.Kind, dark))
			object.TextStyle = font
			object.TextSize = size
			r.pieces = append(r.pieces, syntaxPiece{text: object, x: x, y: y})
			r.objects = append(r.objects, object)
			x += fyne.MeasureText(part, size, font).Width
		}
	}
	r.min = fyne.NewSize(max(maxWidth, x)+6, y+lineHeight+6)
}
func (r *syntaxRenderer) MinSize() fyne.Size { return r.min }
func (r *syntaxRenderer) Layout(fyne.Size) {
	for _, piece := range r.pieces {
		piece.text.Move(fyne.NewPos(piece.x, piece.y))
		piece.text.Resize(piece.text.MinSize())
	}
}
func (r *syntaxRenderer) Objects() []fyne.CanvasObject { return r.objects }
func (r *syntaxRenderer) Refresh() {
	r.update()
	r.Layout(r.layer.Size())
	for _, object := range r.objects {
		object.Refresh()
	}
	canvas.Refresh(r.layer)
}
func (r *syntaxRenderer) Destroy() {}
func tokenColor(kind string, dark bool) color.Color {
	if dark {
		switch kind {
		case "keyword":
			return color.NRGBA{R: 197, G: 134, B: 192, A: 255}
		case "string":
			return color.NRGBA{R: 206, G: 145, B: 120, A: 255}
		case "number":
			return color.NRGBA{R: 181, G: 206, B: 168, A: 255}
		case "comment":
			return color.NRGBA{R: 106, G: 153, B: 85, A: 255}
		default:
			return color.NRGBA{R: 230, G: 230, B: 230, A: 255}
		}
	}
	switch kind {
	case "keyword":
		return color.NRGBA{R: 121, G: 35, B: 223, A: 255}
	case "string":
		return color.NRGBA{R: 163, G: 21, B: 21, A: 255}
	case "number":
		return color.NRGBA{R: 9, G: 134, B: 88, A: 255}
	case "comment":
		return color.NRGBA{R: 0, G: 128, B: 0, A: 255}
	default:
		return color.NRGBA{R: 31, G: 41, B: 55, A: 255}
	}
}
