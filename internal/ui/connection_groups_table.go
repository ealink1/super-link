package ui

import (
	"image/color"
	"strings"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
)

var connectionGroupTableBorder = shellTone{day: color.NRGBA{R: 232, G: 234, B: 240, A: 255}, night: color.NRGBA{R: 84, G: 94, B: 109, A: 255}}

var connectionGroupSelection = shellTone{day: color.NRGBA{R: 238, G: 241, B: 254, A: 255}, night: color.NRGBA{R: 43, G: 51, B: 85, A: 255}}

type connectionGroupsTheme struct{ shellTheme }

func (t connectionGroupsTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNamePrimary {
		return connectionGroupPrimary
	}
	switch name {
	case theme.ColorNameSelection:
		return connectionGroupSelection
	case theme.ColorNameSeparator, theme.ColorNameInputBorder:
		return connectionGroupTableBorder
	}
	return t.shellTheme.Color(name, variant)
}

type connectionGroupCell struct {
	widget.BaseWidget
	label                        *widget.Label
	check                        *widget.Check
	edit, remove                 *shellAlignedButton
	actions                      *fyne.Container
	content                      *fyne.Container
	avatar, statusText           *canvas.Text
	avatarView, statusView       fyne.CanvasObject
	statusBackground, background *canvas.Rectangle
}

func (d *connectionGroups) newCell() fyne.CanvasObject {
	c := &connectionGroupCell{label: widget.NewLabel(""), check: widget.NewCheck("", nil)}
	c.label.Truncation = fyne.TextTruncateEllipsis
	c.avatar = canvas.NewText("", connectionGroupPrimary)
	c.avatar.TextSize = 12
	c.avatar.Alignment = fyne.TextAlignCenter
	c.avatarView = container.NewCenter(shellFixed(groupPanel(container.NewCenter(c.avatar), connectionGroupSelection, 9, 0), 30, 30))
	c.statusText = canvas.NewText("", theme.Color(theme.ColorNamePlaceHolder))
	c.statusText.TextSize = 12
	c.statusText.Alignment = fyne.TextAlignCenter
	c.statusBackground = canvas.NewRectangle(color.Transparent)
	c.statusBackground.CornerRadius = 6
	c.statusView = container.NewCenter(shellFixed(container.NewStack(c.statusBackground, container.NewCenter(c.statusText)), 72, 24))
	c.background = canvas.NewRectangle(color.Transparent)
	c.edit = shellButton("", "pencil", false, nil)
	c.remove = shellButton("", "trash-2", false, nil)
	c.remove.Importance = widget.LowImportance
	if resource, ok := c.remove.Icon.(*shellOutlineIcon); ok {
		resource.shade = theme.ColorNameError
	}
	c.actions = shellHBox(shellFixed(c.edit, 30, 30), shellFixed(c.remove, 30, 30))
	c.content = container.NewStack(c.background, container.NewBorder(nil, widget.NewSeparator(), nil, nil, container.NewStack(container.NewBorder(nil, nil, c.avatarView, nil, container.New(&connectionGroupCellLabelLayout{}, c.label)), c.check, container.NewCenter(c.actions), c.statusView)))
	c.ExtendBaseWidget(c)
	return c
}

func (c *connectionGroupCell) CreateRenderer() fyne.WidgetRenderer {
	return &connectionGroupCellRenderer{widget.NewSimpleRenderer(c.content)}
}

type connectionGroupCellRenderer struct{ fyne.WidgetRenderer }

func (r *connectionGroupCellRenderer) MinSize() fyne.Size { return fyne.NewSize(1, 52) }

func (d *connectionGroups) newTable() *widget.Table {
	table := widget.NewTable(func() (int, int) { return len(d.rows), 6 }, d.newCell, func(id widget.TableCellID, object fyne.CanvasObject) {
		c := object.(*connectionGroupCell)
		c.label.Hide()
		c.check.Hide()
		c.actions.Hide()
		c.avatarView.Hide()
		c.statusView.Hide()
		if id.Row >= len(d.rows) {
			return
		}
		p := d.rows[id.Row]
		c.background.FillColor = color.Transparent
		if d.selected[p.ID] {
			c.background.FillColor = connectionGroupSelection
		}
		c.background.Refresh()
		switch id.Col {
		case 0:
			c.check.OnChanged = nil
			c.check.SetChecked(d.selected[p.ID])
			c.check.OnChanged = func(checked bool) { d.selected[p.ID] = checked; d.table.Refresh(); d.updateMove() }
			c.check.Show()
		case 5:
			c.edit.OnTapped = func() { d.popup.Hide(); d.owner.editProfile(p) }
			c.remove.OnTapped = func() { d.deleteConnection(p) }
			c.actions.Show()
		case 3:
			c.bindStatus(d.statuses[p.ID])
			c.statusView.Show()
		default:
			value := p.Name
			if id.Col == 1 {
				r, _ := utf8.DecodeRuneInString(p.Name)
				c.avatar.Text = strings.ToUpper(string(r))
				c.avatar.Refresh()
				c.avatarView.Show()
			}
			if id.Col == 2 {
				value = connectionAddress(p)
			}
			if id.Col == 4 {
				value = "—"
				if !p.CreatedAt.IsZero() {
					value = p.CreatedAt.Local().Format("2006-01-02 15:04:05")
				}
			}
			c.label.SetText(value)
			c.label.Show()
		}
	})
	table.HideSeparators = true
	table.ShowHeaderRow = true
	table.CreateHeader = d.newCell
	table.UpdateHeader = func(id widget.TableCellID, object fyne.CanvasObject) {
		c := object.(*connectionGroupCell)
		c.label.Hide()
		c.check.Hide()
		c.actions.Hide()
		c.avatarView.Hide()
		c.statusView.Hide()
		if id.Col == 0 {
			all := len(d.rows) > 0
			for _, p := range d.rows {
				all = all && d.selected[p.ID]
			}
			c.check.OnChanged = nil
			c.check.SetChecked(all)
			c.check.OnChanged = func(checked bool) {
				for _, p := range d.rows {
					d.selected[p.ID] = checked
				}
				table.Refresh()
				d.updateMove()
			}
			c.check.Show()
			return
		}
		c.label.TextStyle = fyne.TextStyle{Bold: true}
		c.label.SetText([]string{"", "连接名称", "地址", "状态", "添加时间", "操作"}[id.Col])
		c.label.Show()
	}
	for i, width := range []float32{40, 190, 180, 80, 184, 76} {
		table.SetColumnWidth(i, width)
	}
	return table
}

var connectionGroupSidebar = shellTone{day: color.NRGBA{R: 251, G: 251, B: 253, A: 255}, night: color.NRGBA{R: 30, G: 34, B: 40, A: 255}}

type connectionGroupsTableLayout struct{ dialog *connectionGroups }

func (l *connectionGroupsTableLayout) MinSize([]fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(150, 100)
}
func (l *connectionGroupsTableLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	width := max(280, size.Width-384)
	l.dialog.table.SetColumnWidth(1, width*.5)
	l.dialog.table.SetColumnWidth(2, width*.5)
	objects[0].Move(fyne.Position{})
	objects[0].Resize(fyne.NewSize(size.Width, size.Height))
}

type connectionGroupHeadingTheme struct{ shellTheme }

func (t connectionGroupHeadingTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 17
	}
	return t.shellTheme.Size(name)
}

var connectionGroupPrimary = color.NRGBA{R: 79, G: 110, B: 247, A: 255}
var connectionGroupNeutral = shellTone{day: color.NRGBA{R: 242, G: 244, B: 247, A: 255}, night: color.NRGBA{R: 48, G: 53, B: 61, A: 255}}

func (t connectionGroupsTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameButtonRadius, theme.SizeNameInputRadius, theme.SizeNameSelectionRadius:
		return 10
	case theme.SizeNamePopupRadius:
		return 20
	}
	return t.shellTheme.Size(name)
}
func (c *connectionGroupCell) bindStatus(status application.ConnectionStatus) {
	text := "离线"
	foreground := color.NRGBA{R: 139, G: 147, B: 161, A: 255}
	background := color.NRGBA{R: 241, G: 242, B: 244, A: 255}
	switch status {
	case application.ConnectionConnected:
		text = "在线"
		foreground = color.NRGBA{R: 26, G: 155, B: 82, A: 255}
		background = color.NRGBA{R: 231, G: 246, B: 236, A: 255}
	case application.ConnectionConnecting:
		text = "连接中"
		foreground = connectionGroupPrimary
		background = color.NRGBA{R: 238, G: 241, B: 254, A: 255}
	case application.ConnectionFailed:
		text = "连接失败"
		foreground = color.NRGBA{R: 240, G: 72, B: 62, A: 255}
		background = color.NRGBA{R: 253, G: 236, B: 235, A: 255}
	}
	c.statusText.Text = text
	c.statusText.Color = foreground
	c.statusText.Refresh()
	c.statusBackground.FillColor = background
	c.statusBackground.Refresh()
}

type connectionGroupCellLabelLayout struct{}

func (*connectionGroupCellLabelLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[0].MinSize()
}
func (*connectionGroupCellLabelLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	height := objects[0].MinSize().Height
	objects[0].Move(fyne.NewPos(0, (size.Height-height)/2))
	objects[0].Resize(fyne.NewSize(size.Width, height))
}
