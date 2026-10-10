package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

func groupPanel(object fyne.CanvasObject, fill color.Color, radius, padding float32) *fyne.Container {
	panel := shellPanel(object, fill, radius, padding)
	panel.Objects[0].(*shellPrimitive).stroke = connectionGroupTableBorder
	return panel
}

func (d *connectionGroups) build() {
	colors := connectionGroupsTheme{newShellTheme()}
	close := func() { d.popup.Hide() }
	title := widget.NewLabelWithStyle("管理连接分组", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	resource := shellIcon("settings", false)
	if r, ok := resource.(*shellOutlineIcon); ok {
		r.shade = theme.ColorNameForegroundOnPrimary
	}
	icon := widget.NewIcon(resource)
	iconView := groupPanel(shellFixed(icon, 18, 18), connectionGroupPrimary, 10, 8)
	headLeft := shellHBox(shellFixed(iconView, 34, 34), shellFixed(layout.NewSpacer(), 12, 0), title, shellFixed(layout.NewSpacer(), 6, 0), container.NewThemeOverride(d.total, connectionGroupMutedTheme{newShellTheme()}))
	refresh := shellButton("", "refresh-cw", false, d.load)
	header := shellInset(shellBorder(nil, nil, headLeft, shellHBox(shellFixed(groupPanel(refresh, shellPanelColor, 9, 0), 32, 32), shellFixed(layout.NewSpacer(), 8, 0), shellFixed(groupPanel(shellButton("", "x", false, close), shellPanelColor, 9, 0), 32, 32)), layout.NewSpacer()), 20)
	add := shellButton("新建分组", "plus", true, d.createGroup)
	side := groupPanel(shellBorder(shellVBox(shellFixed(add, 0, 42), noteGroupsGap(14)), nil, nil, nil, container.NewVScroll(d.sidebar)), connectionGroupSidebar, 20, 14)
	dot := canvas.NewCircle(connectionGroupPrimary)
	heading := shellHBox(shellFixed(dot, 7, 7), shellFixed(layout.NewSpacer(), 8, 0), container.NewThemeOverride(d.title, connectionGroupHeadingTheme{newShellTheme()}), container.NewThemeOverride(d.count, connectionGroupMutedTheme{newShellTheme()}))
	search := groupPanel(shellBorder(nil, nil, shellFixed(widget.NewIcon(shellIcon("search", false)), 18, 18), nil, d.search), shellPanelColor, 10, 4)
	tools := shellHBox(shellFixed(search, d.searchWidth(), 36), shellFixed(layout.NewSpacer(), 12, 0), shellFixed(d.order, 138, 36))
	toolbar := shellBorder(nil, nil, nil, tools, heading)
	tableFrame := groupPanel(d.table, shellPanelColor, 13, 1)
	d.tableHost = container.New(&connectionGroupsTableLayout{d}, tableFrame)
	tableView := container.NewStack(d.tableHost, container.NewCenter(d.empty))
	done := shellButton("完成", "", true, close)
	right := shellHBox(shellFixed(d.destination, 180, 38), shellFixed(layout.NewSpacer(), 10, 0), shellFixed(groupPanel(d.move, shellPanelColor, 10, 0), 120, 38), shellFixed(layout.NewSpacer(), 10, 0), shellFixed(done, 72, 38))
	footer := shellInset(shellBorder(nil, nil, d.feedback, right, layout.NewSpacer()), 16)
	contentBody := shellInset(shellBorder(shellVBox(shellFixed(toolbar, 0, 38), noteGroupsGap(16)), nil, nil, nil, tableView), 22)
	main := shellBorder(nil, shellVBox(noteGroupsGap(0), shellLine(), footer), nil, nil, contentBody)
	size := d.owner.Window.Canvas().Size()
	sideWidth := float32(268)
	if size.Width < 1050 {
		sideWidth = 220
	}
	body := shellBorder(nil, nil, shellFixed(side, sideWidth, 0), nil, main)
	content := groupPanel(shellBorder(shellVBox(header, shellLine()), nil, nil, nil, body), shellPanelColor, 20, 0)
	d.popup = widget.NewModalPopUp(container.NewThemeOverride(content, colors), d.owner.Window.Canvas())
	d.scope = container.NewThemeOverride(d.popup, connectionGroupModalTheme{shellModalTheme{shellTheme: newShellTheme()}})
	d.popup.Resize(fyne.NewSize(min(1180, max(720, size.Width-48)), min(760, max(430, size.Height-48))))
}

func (d *connectionGroups) groupItem(name string, count int) fyne.CanvasObject {
	active := name == d.active
	button := shellButton(groupLabel(name), "folder", false, func() {
		d.active = name
		d.selected = map[string]bool{}
		d.search.SetText("")
		d.rebuildSidebar()
		d.filter()
	})
	if option := d.options[name]; option.Color != "" {
		button.SetIcon(coloredIcon("folder", option.Color))
	}
	button.Alignment = widget.ButtonAlignLeading
	badge := widget.NewLabel(fmt.Sprint(count))
	badge.Alignment = fyne.TextAlignCenter
	fill := connectionGroupNeutral
	if active {
		if r, ok := button.Icon.(*shellOutlineIcon); ok {
			r.shade = theme.ColorNamePrimary
		}
		fill = connectionGroupSelection
		button.Importance = widget.LowImportance
	}
	badgeView := groupPanel(badge, fill, 10, 0)
	row := shellBorder(nil, nil, nil, shellFixed(badgeView, 30, 22), container.NewThemeOverride(button, connectionGroupRowTheme{shellTheme: newShellTheme(), active: active}))
	if active {
		row = groupPanel(row, connectionGroupSelection, 10, 8)
	} else {
		row = shellInset(row, 8)
	}
	depth := min(8, connectionGroupDepth(name, d.parents))
	if depth > 0 {
		row = shellBorder(nil, nil, shellFixed(layout.NewSpacer(), float32(depth)*14, 0), nil, row)
	}
	return shellFixed(row, 0, 42)
}

func (d *connectionGroups) searchWidth() float32 {
	if d.owner.Window.Canvas().Size().Width < 1100 {
		return 150
	}
	return 220
}

type connectionGroupModalTheme struct{ shellModalTheme }

func (t connectionGroupModalTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNamePopupRadius {
		return 20
	}
	return t.shellModalTheme.Size(name)
}

type connectionGroupMutedTheme struct{ shellTheme }

func (t connectionGroupMutedTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		return t.shellTheme.Color(theme.ColorNamePlaceHolder, variant)
	}
	return t.shellTheme.Color(name, variant)
}

type connectionGroupRowTheme struct {
	shellTheme
	active bool
}

func (t connectionGroupRowTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameButton {
		return color.Transparent
	}
	if t.active && name == theme.ColorNameForeground {
		return connectionGroupPrimary
	}
	if name == theme.ColorNameHover || name == theme.ColorNamePressed {
		return connectionGroupSelection
	}
	return t.shellTheme.Color(name, variant)
}
