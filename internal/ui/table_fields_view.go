package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"strings"
)

var fieldHeaders = []string{"名称", "类型", "无符号", "主键", "自增", "不是 NULL", "默认", "注释", "操作"}

func (t *tableWorkspace) fieldDesigner(row int) *tableDesigner {
	t.design()
	for _, d := range t.owner.designers {
		if d.profile.ID == t.profile.ID && d.object == t.object && !d.closed {
			if row >= 0 {
				clear(d.selected)
				d.selected[row] = true
				d.grid.Refresh()
			}
			return d
		}
	}
	return nil
}

func (t *tableWorkspace) fieldsView() fyne.CanvasObject {
	grid := widget.NewTable(func() (int, int) { return len(t.page.Info.Columns), len(fieldHeaders) }, func() fyne.CanvasObject {
		label := widget.NewLabel("")
		label.Wrapping = fyne.TextTruncate
		check := widget.NewCheck("", nil)
		check.Disable()
		actions := container.NewHBox(action("", "edit", nil), action("", "trash", nil))
		return container.NewStack(label, check, actions)
	}, func(id widget.TableCellID, item fyne.CanvasObject) {
		box := item.(*fyne.Container)
		label := box.Objects[0].(*widget.Label)
		check := box.Objects[1].(*widget.Check)
		actions := box.Objects[2].(*fyne.Container)
		label.Hide()
		check.Hide()
		actions.Hide()
		c := t.page.Info.Columns[id.Row]
		switch id.Col {
		case 2, 3, 4, 5:
			flags := []bool{strings.Contains(strings.ToLower(c.Type), "unsigned"), c.Key == "PRI", strings.Contains(c.Extra, "auto_increment"), c.Nullable == "NO"}
			check.SetChecked(flags[id.Col-2])
			check.Show()
		case 8:
			actions.Objects[0].(*widget.Button).OnTapped = func() { t.editFieldDefinition(id.Row) }
			actions.Objects[1].(*widget.Button).OnTapped = func() {
				if d := t.fieldDesigner(id.Row); d != nil {
					d.deleteFields()
				}
			}
			if t.profile.ReadOnly || t.profile.Config.Protection.RestrictStructureEdit {
				actions.Objects[1].(*widget.Button).Disable()
			} else {
				actions.Objects[1].(*widget.Button).Enable()
			}
			actions.Show()
		default:
			text := ""
			switch id.Col {
			case 0:
				text = c.Name
			case 1:
				text = c.Type
			case 6:
				if c.Default != nil {
					text = *c.Default
				}
			case 7:
				text = c.Comment
			}
			label.SetText(text)
			label.Show()
		}
	})
	grid.HideSeparators = true
	grid.ShowHeaderRow = true
	grid.CreateHeader = func() fyne.CanvasObject {
		return widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	}
	grid.UpdateHeader = func(id widget.TableCellID, obj fyne.CanvasObject) {
		if id.Col >= 0 && id.Col < len(fieldHeaders) {
			obj.(*widget.Label).SetText(fieldHeaders[id.Col])
		}
	}
	for col, width := range []float32{160, 150, 65, 65, 65, 90, 180, 300, 80} {
		grid.SetColumnWidth(col, width)
	}
	tools := container.NewHBox(action("刷新", "refresh", t.refresh), action("添加字段", "add-row", func() {
		if d := t.fieldDesigner(-1); d != nil {
			d.addField()
		}
	}), action("设计与保存", "table-design", func() { t.fieldDesigner(-1) }), action("复制字段", "copy", func() {
		var lines []string
		for _, c := range t.page.Info.Columns {
			lines = append(lines, c.Name+"\t"+c.Type+"\t"+c.Comment)
		}
		fyne.CurrentApp().Clipboard().SetContent(strings.Join(lines, "\n"))
	}))
	return container.NewBorder(container.NewVBox(tools, widget.NewSeparator()), nil, nil, nil, container.NewThemeOverride(grid, catalogGridTheme{fyne.CurrentApp().Settings().Theme()}))
}

func (t *tableWorkspace) editFieldDefinition(row int) {
	d := t.fieldDesigner(row)
	if d == nil || !d.canEdit() {
		return
	}
	c := d.fields[row].column
	name, kind, def, comment := widget.NewEntry(), widget.NewEntry(), widget.NewEntry(), widget.NewEntry()
	name.SetText(c.Name)
	kind.SetText(c.Type)
	comment.SetText(c.Comment)
	if c.Default != nil {
		def.SetText(*c.Default)
	}
	primary, increment, notNull := widget.NewCheck("主键", nil), widget.NewCheck("自增", nil), widget.NewCheck("不是 NULL", nil)
	primary.SetChecked(c.Key == "PRI")
	increment.SetChecked(strings.Contains(c.Extra, "auto_increment"))
	notNull.SetChecked(c.Nullable == "NO")
	noDefault := widget.NewCheck("无默认值", nil)
	noDefault.SetChecked(c.Default == nil)
	form := widget.NewForm(widget.NewFormItem("名称", name), widget.NewFormItem("类型", kind), widget.NewFormItem("默认", def), widget.NewFormItem("注释", comment))
	modal := dialog.NewCustomConfirm("编辑字段", "暂存", "取消", container.NewVBox(form, container.NewHBox(primary, increment, notNull, noDefault)), func(ok bool) {
		if !ok || !d.canEdit() {
			return
		}
		for col, value := range map[int]string{0: name.Text, 1: kind.Text, 5: def.Text, 6: comment.Text} {
			if err := d.editField(row, col, value, col == 5 && noDefault.Checked); err != nil {
				d.owner.showError(err)
				return
			}
		}
		for col, value := range map[int]bool{2: primary.Checked, 3: increment.Checked, 4: notNull.Checked} {
			text := "false"
			if value {
				text = "true"
			}
			if err := d.editField(row, col, text, false); err != nil {
				d.owner.showError(err)
				return
			}
		}
		d.rebuildFields()
	}, t.owner.Window)
	modal.Resize(fyne.NewSize(540, 360))
	modal.Show()
}
