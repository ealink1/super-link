package ui

import (
	"context"
	"fmt"
	"strings"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

type connectionGroupForm struct {
	parentTargets map[string]string
	manager       *connectionGroups
	popup         *widget.PopUp
	scope         *container.ThemeOverride
	name          *shellFormEntry
	parent        *connectionGroupSelect
	description   *widget.Entry
	counter       *widget.Label
	defaultGroup  *connectionGroupToggle
	colors        []*connectionGroupSwatch
	selectedColor string
	feedback      *widget.Label
	confirm       *connectionGroupSubmitButton
	busy          bool
}

func newConnectionGroupForm(manager *connectionGroups) *connectionGroupForm {
	f := &connectionGroupForm{manager: manager, busy: true, selectedColor: "#4f6ef7"}
	f.name = newShellFormEntry(false)
	f.name.SetPlaceHolder("例如：生产环境、杭州机房")
	f.parent = newConnectionGroupSelect(manager.owner.Window.Canvas())
	f.parent.PlaceHolder = "— 作为顶级分组 —"
	f.description = widget.NewMultiLineEntry()
	f.description.Wrapping = fyne.TextWrapWord
	f.description.SetPlaceHolder("简要说明该分组的用途，便于团队协作识别")
	f.counter = widget.NewLabel("0 / 60")
	f.defaultGroup = newConnectionGroupToggle()
	f.feedback = widget.NewLabel("")
	f.feedback.Wrapping = fyne.TextWrapWord
	f.feedback.Hide()
	f.confirm = newConnectionGroupSubmitButton(f.save)
	f.confirm.Disable()
	f.name.OnChanged = func(string) { f.update() }
	f.description.OnChanged = func(value string) {
		f.counter.SetText(fmt.Sprintf("%d / 60", utf8.RuneCountInString(value)))
		f.update()
	}
	f.name.OnSubmitted = func(string) { f.save() }
	f.build()
	return f
}

func (f *connectionGroupForm) show() {
	f.popup.Show()
	f.manager.owner.Window.Canvas().Focus(f.name)
	f.manager.owner.jobs.run(func(ctx context.Context) (any, error) { return readConnectionGroupSnapshot(ctx, f.manager.owner) }, func(value any, err error) {
		if err != nil {
			f.feedback.SetText(err.Error())
			f.feedback.Show()
			return
		}
		f.busy = false
		data := value.(connectionGroupSnapshot)
		const root = "— 作为顶级分组 —"
		f.parent.Options = []string{root}
		f.parentTargets = map[string]string{root: ""}
		for _, group := range connectionGroupOrder(data.groups, data.parents) {
			label := group
			for {
				if _, exists := f.parentTargets[label]; !exists {
					break
				}
				label += "（分组）"
			}
			f.parent.Options = append(f.parent.Options, label)
			f.parentTargets[label] = group
		}
		f.parent.SetSelected(root)
		f.update()
	})
}

func (f *connectionGroupForm) validation() string {
	length := utf8.RuneCountInString(strings.TrimSpace(f.name.Text))
	if length < 2 || length > 20 {
		return "分组名称须为 2–20 个字符"
	}
	if utf8.RuneCountInString(f.description.Text) > 60 {
		return "描述最多 60 个字符"
	}
	return ""
}

func (f *connectionGroupForm) update() {
	f.confirm.Disable()
	if !f.busy && f.validation() == "" {
		f.confirm.Enable()
	}
	if !f.busy && (f.name.Text != "" || f.feedback.Visible()) {
		f.feedback.SetText(f.validation())
		if f.feedback.Text == "" {
			f.feedback.Hide()
		} else {
			f.feedback.Show()
		}
	}
}

func (f *connectionGroupForm) close() {
	if !f.name.Disabled() {
		f.parent.closePopup()
		f.popup.Hide()
	}
}

func (f *connectionGroupForm) setSaving(saving bool) {
	f.busy = saving
	if saving {
		f.name.Disable()
		f.parent.Disable()
		f.description.Disable()
		f.defaultGroup.Disable()
	} else {
		f.name.Enable()
		f.parent.Enable()
		f.description.Enable()
		f.defaultGroup.Enable()
	}
	for _, swatch := range f.colors {
		if saving {
			swatch.Disable()
		} else {
			swatch.Enable()
		}
	}
	f.update()
}

func (f *connectionGroupForm) save() {
	if f.busy {
		return
	}
	if message := f.validation(); message != "" {
		f.feedback.SetText(message)
		f.feedback.Show()
		return
	}
	name := strings.TrimSpace(f.name.Text)
	parent := f.parentTargets[f.parent.Selected]
	options := domain.ConnectionGroupOptions{Color: f.selectedColor, Description: f.description.Text, Default: f.defaultGroup.Checked}
	f.setSaving(true)
	f.feedback.SetText("正在创建分组…")
	f.feedback.Show()
	f.manager.owner.jobs.run(func(ctx context.Context) (any, error) {
		return nil, f.manager.owner.Profiles.CreateGroupWithOptions(ctx, name, parent, options)
	}, func(_ any, err error) {
		f.setSaving(false)
		if err != nil {
			f.feedback.SetText(err.Error())
			f.feedback.Show()
			return
		}
		f.close()
		f.manager.active = name
		f.manager.search.SetText("")
		f.manager.load()
		f.manager.owner.reload()
	})
}
