package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
)

func TestConnectionGroupFormCreatesChildWithPreferences(t *testing.T) {
	w, p := parityWindow(t)
	if err := w.Profiles.AddGroup(t.Context(), "生产环境"); err != nil {
		t.Fatal(err)
	}
	manager := newConnectionGroups(w)
	manager.show()
	waitUI(t, w)
	f := newConnectionGroupForm(manager)
	f.show()
	waitUI(t, w)
	if !f.confirm.Disabled() {
		t.Fatal("empty name can be submitted")
	}
	f.name.SetText("  国内  ")
	f.parent.SetSelected("生产环境")
	f.description.SetText("  国内生产数据库  ")
	test.Tap(f.colors[1])
	test.Tap(f.defaultGroup)
	test.Tap(f.confirm)
	waitUI(t, w)
	options, err := w.Profiles.GroupOptions(t.Context())
	if err != nil || options["国内"].Color != "#10b981" || options["国内"].Description != "国内生产数据库" || !options["国内"].Default {
		t.Fatal("preferences not persisted", options, err)
	}
	if f.popup.Visible() {
		t.Fatal("successful creation did not close dialog")
	}
	parent := w.sidebar.nodes["connection-group:生产环境"]
	child := w.sidebar.nodes["connection-group:国内"]
	if parent == nil || child == nil || child.parent != parent.id {
		t.Fatal("parent hierarchy not rendered")
	}
	p.ID, p.Revision, p.Group, p.SecretRef = "", 0, "", ""
	created, err := w.Profiles.Save(t.Context(), p)
	if err != nil || created.Group != "国内" {
		t.Fatal("new connection not assigned to default group", err, created.Group)
	}
	editor := &connectionEditor{owner: w, original: p}
	editor.initFields()
	if editor.group.Text != "国内" {
		t.Fatal("connection editor does not show default group", editor.group.Text)
	}
}

func TestConnectionGroupFormDuplicateErrorAndCancel(t *testing.T) {
	w, _ := parityWindow(t)
	if err := w.Profiles.AddGroup(t.Context(), "已有分组"); err != nil {
		t.Fatal(err)
	}
	manager := newConnectionGroups(w)
	manager.show()
	waitUI(t, w)
	f := newConnectionGroupForm(manager)
	f.show()
	waitUI(t, w)
	f.name.SetText("已有分组")
	test.Tap(f.confirm)
	waitUI(t, w)
	if !f.popup.Visible() || !f.feedback.Visible() || f.feedback.Text != "分组名称已存在" {
		t.Fatal("missing inline duplicate feedback", f.feedback.Text)
	}
	f.close()
	if f.popup.Visible() {
		t.Fatal("cancel did not close")
	}
}

func TestConnectionGroupFormValidatesUnicodeAndToggle(t *testing.T) {
	w, _ := parityWindow(t)
	manager := newConnectionGroups(w)
	f := newConnectionGroupForm(manager)
	f.show()
	waitUI(t, w)
	f.name.SetText("华")
	if !f.confirm.Disabled() {
		t.Fatal("one character name allowed")
	}
	if !f.feedback.Visible() || f.feedback.Text != "分组名称须为 2–20 个字符" {
		t.Fatal("missing live validation feedback", f.feedback.Text)
	}
	f.name.SetText(strings.Repeat("华", 20))
	if f.confirm.Disabled() {
		t.Fatal("20 Chinese characters rejected")
	}
	f.name.SetText(strings.Repeat("华", 21))
	if !f.confirm.Disabled() {
		t.Fatal("overlong name allowed")
	}
	f.name.SetText("华东生产")
	f.description.SetText(strings.Repeat("华", 60))
	if f.counter.Text != "60 / 60" || f.confirm.Disabled() {
		t.Fatal("Unicode count incorrect", f.counter.Text)
	}
	f.description.SetText(strings.Repeat("华", 61))
	if !f.confirm.Disabled() {
		t.Fatal("overlong description allowed")
	}
	f.defaultGroup.TypedRune(' ')
	if !f.defaultGroup.Checked {
		t.Fatal("switch cannot be operated with keyboard")
	}
	f.setSaving(true)
	test.Tap(f.defaultGroup)
	test.Tap(f.colors[1])
	if !f.defaultGroup.Checked || f.selectedColor != "#4f6ef7" {
		t.Fatal("saving form can be changed")
	}
	f.setSaving(false)
	f.close()
}

func TestConnectionGroupFormReferenceCapture(t *testing.T) {
	w, _ := parityWindow(t)
	w.Window.Resize(fyne.NewSize(1000, 800))
	manager := newConnectionGroups(w)
	manager.show()
	waitUI(t, w)
	f := newConnectionGroupForm(manager)
	f.show()
	waitUI(t, w)
	f.name.SetText("华东生产")
	captureConnectionIndicators(t, w.Window, "new-connection-group.png")
	w.Window.Resize(fyne.NewSize(700, 520))
	f.popup.Resize(fyne.NewSize(460, 472))
	captureConnectionIndicators(t, w.Window, "new-connection-group-compact.png")
}
