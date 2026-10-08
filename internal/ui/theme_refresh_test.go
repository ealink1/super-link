package ui

import (
	"testing"

	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

func TestCustomTabsAndGridHeadersRefreshThemeColors(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	tab := newDocumentTab("查询", "SQL · demo", true, func() {}, func() {})
	renderer := test.WidgetRenderer(tab).(*documentTabRenderer)
	header := newGridHeader(gridModel{}, &widget.Table{})
	header.bind(1)
	app.Settings().SetTheme(Theme{Dark: true})
	renderer.Refresh()
	test.WidgetRenderer(header).Refresh()
	if renderer.background.FillColor != theme.BackgroundColor() || renderer.title.Color != theme.ForegroundColor() || header.name.Color != theme.ForegroundColor() {
		t.Fatal("custom renderer kept light theme colors in dark mode")
	}
	if renderer.title.Color == renderer.background.FillColor {
		t.Fatal("document title lacks contrast")
	}
	app.Settings().SetTheme(Theme{})
	renderer.Refresh()
	test.WidgetRenderer(header).Refresh()
	if header.name.Color != theme.ForegroundColor() || renderer.indicator.FillColor != documentSelectedGreen {
		t.Fatal("switching back did not restore light theme")
	}
}
