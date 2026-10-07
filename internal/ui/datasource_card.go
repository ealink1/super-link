package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

type sourceCard struct {
	widget.BaseWidget
	descriptor       domain.Descriptor
	colors           sourcePickerTheme
	choose           func()
	hovered, focused bool
}

func newSourceCard(d domain.Descriptor, colors sourcePickerTheme, choose func()) *sourceCard {
	c := &sourceCard{descriptor: d, colors: colors, choose: choose}
	c.ExtendBaseWidget(c)
	return c
}
func (c *sourceCard) Tapped(*fyne.PointEvent) {
	if c.choose != nil {
		c.choose()
	}
}
func (c *sourceCard) MouseIn(*desktop.MouseEvent)    { c.hovered = true; c.Refresh() }
func (c *sourceCard) MouseOut()                      { c.hovered = false; c.Refresh() }
func (c *sourceCard) MouseMoved(*desktop.MouseEvent) {}
func (c *sourceCard) FocusGained()                   { c.focused = true; c.Refresh() }
func (c *sourceCard) FocusLost()                     { c.focused = false; c.Refresh() }
func (c *sourceCard) TypedRune(rune)                 {}
func (c *sourceCard) TypedKey(e *fyne.KeyEvent) {
	if e.Name == fyne.KeyReturn || e.Name == fyne.KeySpace {
		c.Tapped(nil)
	}
}
func (c *sourceCard) CreateRenderer() fyne.WidgetRenderer {
	bg := canvas.NewRectangle(c.colors.shade("panel"))
	bg.CornerRadius = 12
	bg.StrokeWidth = 1
	badge := newDatabaseBadge("db-" + c.descriptor.Key)
	badge.maxSize = 42
	title := canvas.NewText(c.descriptor.Name, c.colors.shade("text"))
	title.TextSize = 14.5
	title.TextStyle.Bold = true
	sub := canvas.NewText("标准连接配置", c.colors.shade("muted"))
	sub.TextSize = 12.5
	if c.descriptor.Key == "sqlite" || c.descriptor.Key == "duckdb" {
		sub.Text = "本地文件连接"
	}
	tag := canvas.NewRectangle(c.colors.shade("tag"))
	tag.CornerRadius = 6
	category := canvas.NewText(sourceCategory(c.descriptor), c.colors.shade("muted"))
	category.TextSize = 11
	r := &sourceCardRenderer{card: c, bg: bg, badge: badge, title: title, sub: sub, tag: tag, category: category}
	r.Refresh()
	return r
}

type sourceCardRenderer struct {
	card                 *sourceCard
	bg, tag              *canvas.Rectangle
	badge                *databaseBadge
	title, sub, category *canvas.Text
}

func (r *sourceCardRenderer) MinSize() fyne.Size { return fyne.NewSize(240, 104) }
func (r *sourceCardRenderer) Layout(size fyne.Size) {
	r.bg.Resize(size)
	r.badge.Move(fyne.NewPos(16, (size.Height-42)/2))
	r.badge.Resize(fyne.NewSquareSize(42))
	r.title.Text = fitText(r.card.descriptor.Name, max(0, size.Width-86), 14.5, r.title.TextStyle)
	r.title.Move(fyne.NewPos(72, 13))
	r.sub.Move(fyne.NewPos(72, 36))
	r.category.Move(fyne.NewPos(80, 65))
	r.tag.Move(fyne.NewPos(72, 62))
	r.tag.Resize(fyne.NewSize(r.category.MinSize().Width+16, 22))
}
func (r *sourceCardRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.bg, r.badge, r.title, r.sub, r.tag, r.category}
}
func (r *sourceCardRenderer) Refresh() {
	r.bg.StrokeColor = r.card.colors.shade("line")
	if r.card.hovered || r.card.focused {
		r.bg.StrokeColor = r.card.colors.shade("hover")
	}
	r.Layout(r.card.Size())
	r.bg.Refresh()
	r.title.Refresh()
}
func (r *sourceCardRenderer) Destroy() {}
