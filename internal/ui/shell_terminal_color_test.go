package ui

import (
	"image/color"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
)

// The day-mode page background measured from a live Shell screen.
var terminalDayBackground = color.NRGBA{R: 0xfa, G: 0xfa, B: 0xf8, A: 0xff}

// x/ansi resolves SGR 30-37 and 90-97 through this VGA palette, which assumes a
// dark screen.
var terminalPalette16 = []color.NRGBA{
	{R: 0x00, G: 0x00, B: 0x00, A: 0xff}, {R: 0x80, G: 0x00, B: 0x00, A: 0xff},
	{R: 0x00, G: 0x80, B: 0x00, A: 0xff}, {R: 0x80, G: 0x80, B: 0x00, A: 0xff},
	{R: 0x00, G: 0x00, B: 0x80, A: 0xff}, {R: 0x80, G: 0x00, B: 0x80, A: 0xff},
	{R: 0x00, G: 0x80, B: 0x80, A: 0xff}, {R: 0xc0, G: 0xc0, B: 0xc0, A: 0xff},
	{R: 0x80, G: 0x80, B: 0x80, A: 0xff}, {R: 0xff, G: 0x00, B: 0x00, A: 0xff},
	{R: 0x00, G: 0xff, B: 0x00, A: 0xff}, {R: 0xff, G: 0xff, B: 0x00, A: 0xff},
	{R: 0x00, G: 0x00, B: 0xff, A: 0xff}, {R: 0xff, G: 0x00, B: 0xff, A: 0xff},
	{R: 0x00, G: 0xff, B: 0xff, A: 0xff}, {R: 0xff, G: 0xff, B: 0xff, A: 0xff},
}

func TestTerminalInkStaysReadableOnDayBackground(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	for index, ink := range terminalPalette16 {
		if contrastAgainst(ink, terminalDayBackground) >= terminalInkContrast {
			continue // Already dark enough for the light page.
		}
		got := terminalNRGBA(terminalInkColor(ink, terminalDayBackground))
		if contrast := contrastAgainst(got, terminalDayBackground); contrast < terminalInkContrast {
			t.Errorf("slot %d %v resolved to %v at %.2f:1", index, ink, got, contrast)
		}
		if ink.R == 0 && ink.G == ink.B && got.R != 0 {
			t.Errorf("slot %d lost its hue: %v -> %v", index, ink, got)
		}
	}
	readable := color.NRGBA{R: 0, G: 0, B: 0x80, A: 0xff}
	if got := terminalNRGBA(terminalInkColor(readable, terminalDayBackground)); got != readable {
		t.Errorf("a readable ink was rewritten: %v -> %v", readable, got)
	}
}

func TestTerminalInkKeepsNightPaletteAsSent(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{Dark: true})
	background := terminalNRGBA((Theme{Dark: true}).Color(theme.ColorNameBackground, theme.VariantLight))
	for index, ink := range terminalPalette16 {
		if got := terminalNRGBA(terminalInkColor(ink, background)); got != ink {
			t.Errorf("night slot %d changed from %v to %v", index, ink, got)
		}
	}
}

func TestTerminalInkFollowsTheCellBackground(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	cyan := color.NRGBA{R: 0, G: 0xff, B: 0xff, A: 0xff}
	// Reverse video puts the page color on top of the program's ink, so the
	// text has to darken against that ink rather than against the page.
	page := terminalDayBackground
	got := terminalNRGBA(terminalInkColor(page, cyan))
	if got == page || contrastAgainst(got, cyan) < terminalInkContrast {
		t.Fatalf("reversed %v on %v stayed unreadable: %v", page, cyan, got)
	}
	if clear := (color.NRGBA{}); terminalInkColor(clear, cyan) != color.Color(clear) {
		t.Error("transparent ink was painted")
	}
}

// A Shell screen rendering vim's day-mode output must not stay at the 1.2:1 the
// VGA palette gives bright cyan on a light background.
func TestTerminalRendererResolvesDayInk(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	surface := newTerminalSurface(func(int, int) {})
	defer surface.dispose()
	window := app.NewWindow("day ink")
	window.SetContent(surface)
	window.Resize(fyne.NewSize(800, 400))
	window.Show()
	defer window.Close()
	surface.feed([]byte("\x1b[96mGeoLite2Path\x1b[0m"))
	renderer := test.WidgetRenderer(surface).(*terminalRenderer)
	renderer.Refresh()
	ink := terminalNRGBA(renderer.pool[0].Color)
	if contrast := contrastAgainst(ink, terminalNRGBA(renderer.backgrounds[0].FillColor)); contrast < terminalInkContrast {
		t.Fatalf("rendered ink %v is %.2f:1 against its cell", ink, contrast)
	}
}

func contrastAgainst(ink, background color.NRGBA) float64 {
	return contrastRatio(relativeLuminance(ink), relativeLuminance(background))
}
