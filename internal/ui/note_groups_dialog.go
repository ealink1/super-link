package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

type noteGroupsDialog struct {
	note                 *noteWorkspace
	popup                *widget.PopUp
	scope                *container.ThemeOverride
	createName, editName *widget.Entry
	selected             *widget.Select
	count, feedback      *widget.Label
	add, rename, remove  *shellAlignedButton
}

func newNoteGroupsDialog(n *noteWorkspace) *noteGroupsDialog {
	d := &noteGroupsDialog{note: n}
	d.createName, d.editName = widget.NewEntry(), widget.NewEntry()
	d.createName.SetPlaceHolder("例如：工作记录、学习笔记")
	d.editName.SetPlaceHolder("输入新的分组名称")
	d.count, d.feedback = widget.NewLabel(""), widget.NewLabel("")
	d.feedback.Wrapping = fyne.TextWrapWord
	d.selected = widget.NewSelect(nil, func(string) { d.selectGroup() })
	d.selected.PlaceHolder = "选择要管理的分组"
	d.add = shellButton("新建分组", "plus", true, d.create)
	d.rename = shellButton("保存名称", "", false, d.renameGroup)
	d.remove = shellButton("移除分组", "trash-2", false, d.removeGroup)
	d.remove.Importance = widget.DangerImportance
	if icon, ok := d.remove.Icon.(*shellOutlineIcon); ok {
		icon.shade = theme.ColorNameForegroundOnError
	}
	d.createName.OnChanged = func(string) { d.updateActions() }
	d.editName.OnChanged = func(string) { d.updateActions() }
	d.createName.OnSubmitted = func(string) {
		if !d.add.Disabled() {
			d.create()
		}
	}
	d.editName.OnSubmitted = func(string) {
		if !d.rename.Disabled() {
			d.renameGroup()
		}
	}
	d.refresh("")
	d.build()
	return d
}

func (d *noteGroupsDialog) build() {
	heading := shellVBox(
		shellText("笔记分组", 19, true, shellTextColor),
		noteGroupsGap(6), shellText("整理笔记，让内容更容易找到。", 12, false, shellMutedColor),
	)
	header := shellInset(shellBorder(nil, nil, nil, shellFixed(shellButtonView(shellButton("", "x", false, d.close)), 32, 32), heading), 24)
	createRow := shellBorder(nil, nil, nil, shellHBox(shellFixed(layout.NewSpacer(), 12, 0), shellFixed(shellButtonView(d.add), 100, 38)), shellFixed(d.createName, 0, 38))
	createCard := shellPanel(shellVBox(shellText("新建分组", 13, true, shellTextColor), noteGroupsGap(12), createRow), shellPanelColor, 10, 16)
	selection := shellFixed(d.selected, 0, 38)
	editRow := shellBorder(nil, nil, nil, shellHBox(shellFixed(layout.NewSpacer(), 12, 0), shellFixed(shellOutlined(d.rename), 100, 38)), shellFixed(d.editName, 0, 38))
	removeRow := shellBorder(nil, nil, shellLabel(d.count, 11), shellFixed(shellButtonView(d.remove), 100, 34), layout.NewSpacer())
	manageCard := shellPanel(shellVBox(shellText("管理已有分组", 13, true, shellTextColor), noteGroupsGap(12), selection, noteGroupsGap(12), editRow, noteGroupsGap(12), removeRow), shellPanelColor, 10, 16)
	body := shellVBox(createCard, noteGroupsGap(16), manageCard, noteGroupsGap(10), shellLabel(d.feedback, 12))
	info := widget.NewLabel("移除分组不会删除笔记，笔记将归入默认分组。")
	info.Wrapping = fyne.TextWrapWord
	footer := shellInset(shellBorder(nil, nil, nil, shellHBox(shellFixed(layout.NewSpacer(), 16, 0), shellFixed(shellButtonView(shellButton("完成", "", true, d.close)), 80, 38)), shellLabel(info, 11)), 24)
	content := shellPanel(shellBorder(header, shellVBox(shellLine(), footer), nil, nil, container.NewVScroll(shellInset(body, 24))), shellBackground, 14, 0)
	d.popup = widget.NewModalPopUp(container.NewThemeOverride(content, newShellTheme()), d.note.owner.Window.Canvas())
	d.scope = container.NewThemeOverride(d.popup, shellModalTheme{shellTheme: newShellTheme()})
	size := d.note.owner.Window.Canvas().Size()
	d.popup.Resize(fyne.NewSize(min(540, max(320, size.Width-48)), min(570, max(320, size.Height-48))))
}

func noteGroupsGap(height float32) fyne.CanvasObject {
	return shellFixed(layout.NewSpacer(), 0, height)
}

func (d *noteGroupsDialog) show() {
	d.popup.Show()
	d.note.owner.Window.Canvas().Focus(d.createName)
}

func (d *noteGroupsDialog) close() { d.popup.Hide() }

func (d *noteGroupsDialog) refresh(preferred string) {
	d.selected.Options = make([]string, 0, len(d.note.book.Groups))
	for _, group := range d.note.book.Groups {
		d.selected.Options = append(d.selected.Options, group.Name)
	}
	d.selected.ClearSelected()
	if len(d.selected.Options) > 0 {
		d.selected.Enable()
		d.selected.PlaceHolder = "选择要管理的分组"
		if preferred == "" {
			preferred = d.selected.Options[0]
		}
		d.selected.SetSelected(preferred)
	} else {
		d.selected.Disable()
		d.selected.PlaceHolder = "暂无自定义分组"
	}
	d.selected.Refresh()
	d.selectGroup()
}

func (d *noteGroupsDialog) selectGroup() {
	d.editName.SetText(d.selected.Selected)
	if d.selected.Selected == "" {
		d.editName.Disable()
		d.count.SetText("新建分组后即可在这里管理")
	} else {
		d.editName.Enable()
		count := 0
		for _, group := range d.note.book.Groups {
			if group.Name != d.selected.Selected {
				continue
			}
			for _, note := range d.note.book.Notes {
				if note.GroupID == group.ID && !note.Deleted {
					count++
				}
			}
		}
		d.count.SetText(fmt.Sprintf("此分组有 %d 篇笔记", count))
	}
	d.updateActions()
}

func (d *noteGroupsDialog) updateActions() {
	d.add.Enable()
	if strings.TrimSpace(d.createName.Text) == "" {
		d.add.Disable()
	}
	d.rename.Enable()
	d.remove.Enable()
	if d.selected.Selected == "" {
		d.remove.Disable()
	}
	if d.selected.Selected == "" || strings.TrimSpace(d.editName.Text) == "" || strings.TrimSpace(d.editName.Text) == d.selected.Selected {
		d.rename.Disable()
	}
}

func (d *noteGroupsDialog) create() {
	name := strings.TrimSpace(d.createName.Text)
	if err := d.note.addGroup(name); err != nil {
		d.feedback.SetText(err.Error())
		return
	}
	d.createName.SetText("")
	d.refresh(name)
	d.feedback.SetText("已新建分组")
}

func (d *noteGroupsDialog) renameGroup() {
	name := strings.TrimSpace(d.editName.Text)
	if err := d.note.renameGroup(d.selected.Selected, name); err != nil {
		d.feedback.SetText(err.Error())
		return
	}
	d.refresh(name)
	d.feedback.SetText("分组名称已更新")
}

func (d *noteGroupsDialog) removeGroup() {
	if d.selected.Selected == "" {
		return
	}
	d.note.removeGroup(d.selected.Selected)
	d.refresh("")
	d.feedback.SetText("分组已移除，笔记已保留")
}
