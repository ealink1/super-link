package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

// Preserve Fyne's keyboard navigation, disabled actions and popup dismissal.
func showContextMenu(menu *fyne.Menu, c fyne.Canvas, position fyne.Position) {
	for _, item := range menu.Items {
		switch item.Label {
		case "新建查询":
			item.Icon = theme.DocumentCreateIcon()
		case "新建库":
			item.Icon = icon("database")
		case "刷新":
			item.Icon = theme.ViewRefreshIcon()
		case "编辑连接", "设计表":
			item.Icon = theme.DocumentCreateIcon()
		case "断开连接":
			item.Icon = theme.LogoutIcon()
		case "删除连接":
			item.Icon = theme.DeleteIcon()
		case "打开工作台", "查看数据", "新建表":
			item.Icon = icon("table")
		case "复制名称", "复制结构", "复制 INSERT 模板":
			item.Icon = theme.ContentCopyIcon()
		case "导出数据":
			item.Icon = icon("export")
		case "DDL":
			item.Icon = icon("sql-doc")
		}
	}
	popup := widget.NewPopUpMenu(menu, c)
	if popup == nil {
		return
	}
	container.NewThemeOverride(popup, contextMenuTheme{})
	popup.Resize(fyne.NewSize(max(180, popup.MinSize().Width), popup.MinSize().Height))
	popup.ShowAtPosition(position)
}

type contextMenuTheme struct{ Theme }

func (t contextMenuTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameText:
		return 13
	case theme.SizeNameMenuRadius:
		return 8
	case theme.SizeNameInlineIcon:
		return 16
	case theme.SizeNameInnerPadding:
		return 8
	case theme.SizeNamePadding:
		return 4
	}
	return fyne.CurrentApp().Settings().Theme().Size(name)
}
func (t contextMenuTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	current := fyne.CurrentApp().Settings().Theme()
	dark := variant == theme.VariantDark
	if value, ok := current.(Theme); ok {
		dark = value.Dark
	}
	if dark {
		switch name {
		case theme.ColorNameMenuBackground:
			return color.NRGBA{R: 36, G: 39, B: 44, A: 255}
		case theme.ColorNameHover, theme.ColorNameFocus:
			return color.NRGBA{R: 44, G: 65, B: 94, A: 255}
		}
	} else {
		switch name {
		case theme.ColorNameMenuBackground:
			return color.White
		case theme.ColorNameForeground:
			return color.NRGBA{R: 51, G: 51, B: 51, A: 255}
		case theme.ColorNameHover, theme.ColorNameFocus:
			return color.NRGBA{R: 232, G: 240, B: 254, A: 255}
		}
	}
	if name == theme.ColorNameShadow {
		return color.NRGBA{A: 20}
	}
	return current.Color(name, variant)
}
