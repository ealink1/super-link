package ui

import (
	"context"
	"image/color"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

func TestAppearancePreservesQueryFormAndTerminalContents(t *testing.T) {
	w := shellTestWindow(t)
	w.Engine.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return uiFixtureClient{}, nil }
	p, err := w.Profiles.Save(context.Background(), domain.Profile{Name: "theme fixture", ReadOnly: true, Config: connection.ConnectionConfig{Type: "sqlite", Host: ":memory:"}})
	if err != nil {
		t.Fatal(err)
	}
	w.newQuery(p)
	w.switcher.selectMode(1)
	waitUI(t, w)
	sqlItem := w.tabs.Selected()
	sql := w.workspaces[sqlItem]
	sql.editor.SetText("SELECT '未执行的主题测试';")
	s := w.switcher.shell
	pane := s.newPane("screen fixture")
	pane.terminal.feed([]byte("\x1b[31;44mRED\x1b[0m plain / 中文"))
	terminal := pane.terminal
	renderer := test.WidgetRenderer(terminal).(*terminalRenderer)
	ansiBackground := renderer.backgrounds[0].FillColor
	content := terminal.emulator.String()
	editor := newShellHostEditor(s, domain.ShellHost{Port: 22, User: "tester", Remember: true})
	editor.name.SetText("未保存的主机")
	editor.password.SetText("fixture-password")
	editor.modal.popup.Show()
	check := w.appearanceCheck()
	for _, dark := range []bool{true, false, true} {
		w.setDark(dark)
		if check.Checked != dark || w.switcher.themeButton.Text != map[bool]string{true: "夜间", false: "日间"}[dark] {
			t.Fatal("theme controls are out of sync")
		}
		if terminal.emulator.String() != content || s.panes[pane.item] != pane || editor.password.Text != "fixture-password" || editor.name.Text != "未保存的主机" {
			t.Fatal("theme change replaced session or form contents")
		}
		for _, name := range []fyne.ThemeColorName{theme.ColorNameBackground, theme.ColorNameForeground, theme.ColorNamePrimary, theme.ColorNameInputBackground, theme.ColorNameInputBorder} {
			want := (Theme{Dark: dark}).Color(name, theme.VariantLight)
			if !sameAppearanceColor(s.content.Theme.Color(name, theme.VariantLight), want) || !sameAppearanceColor(theme.CurrentForWidget(editor.password).Color(name, theme.VariantLight), want) {
				t.Fatalf("Shell or open form diverged from SQL: dark=%v color=%s", dark, name)
			}
		}
		if !sameAppearanceColor(renderer.background.FillColor, (Theme{Dark: dark}).Color(theme.ColorNameBackground, theme.VariantLight)) || !sameAppearanceColor(renderer.pool[1].Color, (Theme{Dark: dark}).Color(theme.ColorNameForeground, theme.VariantLight)) {
			t.Fatal("terminal kept the previous default colors")
		}
		// Night keeps explicit ANSI colors exactly as the program sent them;
		// day mode only moves the ink away from its own cell background.
		ansiInk := color.NRGBA{R: 0x80, A: 0xff}
		if !sameAppearanceColor(renderer.backgrounds[0].FillColor, ansiBackground) {
			t.Fatal("theme change altered the explicit ANSI background")
		}
		if dark {
			if !sameAppearanceColor(renderer.pool[0].Color, ansiInk) {
				t.Fatal("night mode altered an explicit ANSI color")
			}
		} else if sameAppearanceColor(renderer.pool[0].Color, ansiInk) {
			t.Fatal("day mode left red on blue at the dark-screen palette")
		} else if contrastAgainst(terminalNRGBA(renderer.pool[0].Color), terminalNRGBA(ansiBackground)) < contrastAgainst(ansiInk, terminalNRGBA(ansiBackground)) {
			t.Fatal("day mode made the ANSI ink less readable")
		}
		w.switcher.selectMode(0)
		if w.tabs.Selected() != sqlItem || w.workspaces[sqlItem] != sql || sql.editor.Text != "SELECT '未执行的主题测试';" {
			t.Fatal("theme or workspace switch lost SQL input")
		}
		w.switcher.selectMode(1)
	}
	editor.modal.hide()
	pane.stop()
	waitUI(t, w)
}

func TestAppearanceLastChoicePersistsAndRestores(t *testing.T) {
	w := shellTestWindow(t)
	for _, dark := range []bool{true, false, true, false, true} {
		w.setDark(dark)
	}
	waitUI(t, w)
	value, err := w.Store.Setting(context.Background(), appearanceSettingKey)
	if err != nil || value != "dark" {
		t.Fatal("rapid toggles did not persist the last choice", value, err)
	}
	check := w.appearanceCheck()
	test.Tap(check)
	waitUI(t, w)
	if w.dark || w.switcher.themeButton.Text != "日间" {
		t.Fatal("appearance settings did not update the titlebar")
	}
	w.setDark(true)
	waitUI(t, w)
	reloaded := appearanceReloadWindow(t, w)
	if !reloaded.dark || reloaded.switcher.themeButton.Text != "夜间" {
		t.Fatal("stored theme did not restore before workspace readiness")
	}
	reloaded.switcher.selectMode(1)
	waitUI(t, reloaded)
	if !sameAppearanceColor(reloaded.switcher.shell.content.Theme.Color(theme.ColorNameBackground, theme.VariantLight), (Theme{Dark: true}).Color(theme.ColorNameBackground, theme.VariantLight)) {
		t.Fatal("lazily opened Shell did not inherit restored mode")
	}
}

func TestAppearanceStartupReadDoesNotOverrideUserAndExitFlushes(t *testing.T) {
	w := shellTestWindow(t)
	if err := w.Store.SetSetting(context.Background(), appearanceSettingKey, "dark"); err != nil {
		t.Fatal(err)
	}
	reloaded := newTestWindow(t, w.App, appearanceDependencies(w))
	reloaded.setDark(true)
	reloaded.setDark(false)
	reloaded.Window.Show()
	waitReady(t, reloaded)
	defer closeAppearanceWindow(reloaded)
	if reloaded.dark {
		t.Fatal("stale startup read overwrote the user's latest choice")
	}
	reloaded.setDark(true)
	reloaded.setDark(false)
	if err := reloaded.FlushAfterRun(); err != nil {
		t.Fatal(err)
	}
	value, err := w.Store.Setting(context.Background(), appearanceSettingKey)
	if err != nil || value != "light" {
		t.Fatal("event-loop exit lost the final theme choice", value, err)
	}
}

func TestShellPrimitiveRefreshResolvesNewConcreteColors(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	panel := shellRectangle(shellPanelColor, 8, shellBorderColor)
	label := shellText("共同主题", 13, false, shellTextColor).(*shellPrimitive)
	for _, dark := range []bool{true, false} {
		w.setDark(dark)
		panel.Refresh()
		label.Refresh()
		if _, dynamic := label.fill.(shellColor); !dynamic {
			t.Fatal("missing semantic role")
		}
		if _, dynamic := test.WidgetRenderer(label).Objects()[0].(*canvas.Text).Color.(shellColor); dynamic {
			t.Fatal("dynamic role leaked into glyph texture cache")
		}
		if !sameAppearanceColor(test.WidgetRenderer(label).Objects()[0].(*canvas.Text).Color, (Theme{Dark: dark}).Color(theme.ColorNameForeground, theme.VariantLight)) {
			t.Fatal("text kept stale theme color")
		}
	}
	waitUI(t, w)
}

func appearanceDependencies(w *Window) Dependencies {
	return Dependencies{Profiles: w.Profiles, Engine: w.Engine, Drivers: w.Drivers, Releases: w.Releases, Root: w.Root, Version: "test", Close: func() error { return nil }}
}

func appearanceReloadWindow(t *testing.T, w *Window) *Window {
	t.Helper()
	reloaded := newTestWindow(t, w.App, appearanceDependencies(w))
	reloaded.Window.Show()
	waitReady(t, reloaded)
	t.Cleanup(func() { closeAppearanceWindow(reloaded) })
	return reloaded
}

func closeAppearanceWindow(w *Window) {
	w.switcher.stop()
	w.jobs.stop()
	w.Window.SetCloseIntercept(nil)
	w.Window.Close()
}

func sameAppearanceColor(a, b color.Color) bool {
	return color.NRGBAModel.Convert(a) == color.NRGBAModel.Convert(b)
}
