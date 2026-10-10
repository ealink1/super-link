package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

func (f *connectionGroupForm) build() {
	title := shellVBox(shellText("新建分组", 16.5, true, shellTextColor), noteGroupsGap(3), shellText("用于归类与管理数据库连接", 12.5, false, shellMutedColor))
	badge := container.NewStack(groupGradient("#4f6ef7", "#7b93ff", 13), container.NewCenter(shellFixed(widget.NewIcon(groupFormIcon("plus")), 18, 18)))
	close := shellFixed(groupPanel(shellButton("", "x", false, f.close), shellPanelColor, 9, 0), 32, 32)
	header := shellBorder(nil, nil, shellHBox(shellFixed(badge, 36, 36), shellFixed(layout.NewSpacer(), 12, 0), title), close, layout.NewSpacer())
	header = shellBorder(shellFixed(layout.NewSpacer(), 22, 22), nil, nil, nil, header)
	header = shellBorder(nil, nil, shellFixed(layout.NewSpacer(), 24, 0), shellFixed(layout.NewSpacer(), 24, 0), header)
	fields := shellVBox(
		groupFormField("分组名称", "2–20 个字符", true, shellFixed(f.name, 0, 44)), noteGroupsGap(18),
		groupFormField("父级分组", "选填", false, shellFixed(f.parent, 0, 44)), noteGroupsGap(18),
		groupFormField("标识色", "", false, f.colorPicker()), noteGroupsGap(18),
		f.descriptionField(), noteGroupsGap(18), f.defaultRow(), noteGroupsGap(8),
		container.NewThemeOverride(f.feedback, connectionGroupErrorTheme{connectionGroupFormTheme{newShellTheme()}}),
	)
	body := shellBorder(shellFixed(layout.NewSpacer(), 0, 22), shellFixed(layout.NewSpacer(), 0, 6), shellFixed(layout.NewSpacer(), 24, 0), shellFixed(layout.NewSpacer(), 24, 0), fields)
	cancel := shellFixed(groupPanel(shellButton("取消", "", false, f.close), shellPanelColor, 11, 0), 72, 40)
	actions := shellHBox(cancel, shellFixed(layout.NewSpacer(), 12, 0), shellFixed(container.NewThemeOverride(f.confirm, connectionGroupSubmitTheme{connectionGroupFormTheme{newShellTheme()}}), 100, 40))
	footer := shellBorder(shellFixed(layout.NewSpacer(), 0, 18), shellFixed(layout.NewSpacer(), 0, 24), shellFixed(layout.NewSpacer(), 24, 0), shellFixed(layout.NewSpacer(), 24, 0), shellBorder(nil, nil, nil, actions, layout.NewSpacer()))
	content := groupPanel(shellBorder(header, footer, nil, nil, container.NewVScroll(body)), shellPanelColor, 20, 0)
	f.popup = widget.NewModalPopUp(container.NewThemeOverride(content, connectionGroupFormTheme{newShellTheme()}), f.manager.owner.Window.Canvas())
	f.scope = container.NewThemeOverride(f.popup, connectionGroupFormModalTheme{connectionGroupModalTheme{shellModalTheme{shellTheme: newShellTheme()}}})
	size := f.manager.owner.Window.Canvas().Size()
	f.popup.Resize(fyne.NewSize(min(460, max(340, size.Width-48)), min(650, max(380, size.Height-48))))
}

func groupFormField(label, hint string, required bool, control fyne.CanvasObject) fyne.CanvasObject {
	heading := shellHBox(shellText(label, 13, false, shellTextColor))
	if required {
		heading.Add(shellFixed(layout.NewSpacer(), 6, 0))
		heading.Add(shellText("*", 13, false, shellColor(theme.ColorNameError)))
	}
	row := shellBorder(nil, nil, heading, shellText(hint, 11.5, false, shellMutedColor), layout.NewSpacer())
	return shellVBox(row, noteGroupsGap(7), control)
}

func (f *connectionGroupForm) descriptionField() fyne.CanvasObject {
	f.counter.Alignment = fyne.TextAlignTrailing
	count := container.NewThemeOverride(f.counter, connectionGroupCounterTheme{connectionGroupFormTheme{newShellTheme()}, f})
	return shellVBox(groupFormField("描述", "选填", false, shellFixed(f.description, 0, 80)), shellFixed(count, 0, 18))
}

func (f *connectionGroupForm) defaultRow() fyne.CanvasObject {
	info := shellVBox(shellText("默认分组", 13.5, false, shellTextColor), noteGroupsGap(2), shellText("新建的连接将自动归入此分组", 12, false, shellMutedColor))
	row := shellBorder(nil, nil, nil, container.NewCenter(shellFixed(f.defaultGroup, 42, 24)), info)
	return shellFixed(groupPanel(row, connectionGroupSidebar, 12, 14), 0, 64)
}

func (f *connectionGroupForm) colorPicker() fyne.CanvasObject {
	palette := []struct{ start, end, text, icon string }{
		{"#4f6ef7", "#7b93ff", "A", ""}, {"#10b981", "#34d399", "", "check"},
		{"#f59e0b", "#fbbf24", "$", ""}, {"#ef4444", "#f87171", "!", ""},
		{"#8b5cf6", "#a78bfa", "", "shield"}, {"#0ea5e9", "#38bdf8", "", "database"},
	}
	row := shellHBox()
	for index, item := range palette {
		swatch := newConnectionGroupSwatch(item.start, item.end, item.text, item.icon)
		swatch.active = index == 0
		swatch.OnTapped = func() {
			f.selectedColor = item.start
			for _, choice := range f.colors {
				choice.active = choice == swatch
				choice.Refresh()
			}
		}
		f.colors = append(f.colors, swatch)
		if index > 0 {
			row.Add(shellFixed(layout.NewSpacer(), 4, 0))
		}
		// The 3 px inset leaves room for the selection outline around each 42 px swatch.
		row.Add(shellFixed(swatch, 48, 48))
	}
	return row
}

type connectionGroupFormTheme struct{ shellTheme }

type connectionGroupFormModalTheme struct{ connectionGroupModalTheme }

func (t connectionGroupFormModalTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameShadow {
		return color.NRGBA{R: 16, G: 24, B: 40, A: 87}
	}
	return t.connectionGroupModalTheme.Color(name, variant)
}

type connectionGroupSubmitTheme struct{ connectionGroupFormTheme }

func (t connectionGroupSubmitTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNamePrimary {
		return color.Transparent
	}
	return t.connectionGroupFormTheme.Color(name, variant)
}

func (t connectionGroupFormTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	switch name {
	case theme.ColorNamePrimary:
		return connectionGroupPrimary
	case theme.ColorNameInputBorder:
		return connectionGroupTableBorder
	case theme.ColorNameSelection:
		return connectionGroupSelection
	}
	return t.shellTheme.Color(name, variant)
}
func (t connectionGroupFormTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInputRadius, theme.SizeNameButtonRadius:
		return 11
	case theme.SizeNamePopupRadius:
		return 20
	case theme.SizeNameText:
		return 14
	}
	return t.shellTheme.Size(name)
}

type connectionGroupErrorTheme struct{ connectionGroupFormTheme }

func (t connectionGroupErrorTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		return t.shellTheme.Color(theme.ColorNameError, variant)
	}
	return t.connectionGroupFormTheme.Color(name, variant)
}
func (t connectionGroupErrorTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 11.5
	}
	return t.connectionGroupFormTheme.Size(name)
}

type connectionGroupCounterTheme struct {
	connectionGroupFormTheme
	form *connectionGroupForm
}

func (t connectionGroupCounterTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		if len([]rune(t.form.description.Text)) > 50 {
			return t.shellTheme.Color(theme.ColorNameError, variant)
		}
		return t.shellTheme.Color(theme.ColorNamePlaceHolder, variant)
	}
	return t.connectionGroupFormTheme.Color(name, variant)
}
func (t connectionGroupCounterTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 11.5
	}
	return t.connectionGroupFormTheme.Size(name)
}
