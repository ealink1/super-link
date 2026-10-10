package ui

import (
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"strings"
	"testing"
)

func TestSourcePickerFilterAndChoose(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	chosen := ""
	p := newSourcePicker(false, "0.1.0", func(d domain.Descriptor) { chosen = d.Key }, func() {})
	window := app.NewWindow("Source picker")
	defer window.Close()
	window.SetContent(p.content)
	window.Resize(fyne.NewSize(1180, 780))
	window.Show()
	if len(p.cards.Objects) != len(domain.Catalog()) {
		t.Fatal("initial catalog is incomplete")
	}
	p.search.SetText("  MYSQL ")
	if len(p.cards.Objects) != 1 {
		t.Fatal("search must match normalized database names")
	}
	card := p.cards.Objects[0].(*sourceCard)
	test.Tap(card)
	if chosen != "mysql" {
		t.Fatal("selection changed its database identity")
	}
	p.nav.Select(3)
	if len(p.cards.Objects) != 0 || !p.empty.Visible() {
		t.Fatal("category and query filters must combine with an empty state")
	}
	p.search.SetText("")
	if len(p.cards.Objects) != 3 || p.empty.Visible() {
		t.Fatal("clearing search must restore the active category")
	}
	p.nav.Select(0)
	window.Resize(fyne.NewSize(760, 620))
	p.content.Refresh()
	for _, o := range p.cards.Objects {
		if o.Position().X+o.Size().Width > p.cards.Size().Width+1 {
			t.Fatal("card exceeds grid width")
		}
	}
}

func TestSourcePickerRender(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	window := app.NewWindow("Source picker")
	defer window.Close()
	window.SetPadded(false)
	for _, dark := range []bool{false, true} {
		p := newSourcePicker(dark, "0.1.0", func(domain.Descriptor) {}, func() {})
		window.SetContent(p.content)
		for _, size := range []fyne.Size{fyne.NewSize(1180, 780), fyne.NewSize(760, 620)} {
			window.Resize(size)
			window.Show()
			p.content.Refresh()
			last := p.cards.Objects[len(p.cards.Objects)-1]
			bottom := last.Position().Y + last.Size().Height
			if p.cards.Size().Height < bottom || p.cards.Size().Height > bottom+1 {
				t.Fatalf("grid height %v differs from last row bottom %v at %v", p.cards.Size().Height, bottom, size)
			}
			captureConnectionIndicators(t, window, fmt.Sprintf("source-picker-%v-%d.png", dark, int(size.Width)))
		}
	}
}

func TestSourcePickerHasRoundedOuterPopupAndNoVersionFooter(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	window := app.NewWindow("picker")
	defer window.Close()
	window.Resize(fyne.NewSize(900, 700))
	window.Show()
	picker := newSourcePicker(false, "0.1.0", func(domain.Descriptor) {}, func() {})
	walkUpdateDialog(picker.content, func(object fyne.CanvasObject) {
		if text, ok := object.(*canvas.Text); ok && strings.HasPrefix(text.Text, "SuperLink v") {
			t.Fatal("version footer remains")
		}
	})
	popup := widget.NewModalPopUp(picker.content, window.Canvas())
	container.NewThemeOverride(popup, picker.colors)
	popup.Resize(fyne.NewSize(760, 620))
	popup.Show()
	defer popup.Hide()
	background := test.WidgetRenderer(popup).Objects()[0].(*canvas.Rectangle)
	if background.CornerRadius != 20 {
		t.Fatal("outer popup corners are not rounded", background.CornerRadius)
	}
	view := picker.content.Content.(*fyne.Container)
	if view.Objects[0].(*canvas.Rectangle).CornerRadius != background.CornerRadius {
		t.Fatal("square content masks rounded popup")
	}
}
