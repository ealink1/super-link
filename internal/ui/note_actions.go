package ui

import (
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/google/uuid"
)

func (n *noteWorkspace) refreshGroups() {
	options := []string{"默认分组"}
	for _, g := range n.book.Groups {
		options = append(options, g.Name)
	}
	n.groupPicker.Options = options
	if note := n.current(); note != nil {
		n.binding = true
		n.groupPicker.SetSelected(n.groupName(note.GroupID))
		n.binding = false
	}
	n.groupPicker.Refresh()
	n.filter()
}
func (n *noteWorkspace) showMore() {
	note := n.current()
	if note == nil {
		return
	}
	text := "移到回收站"
	if note.Deleted {
		text = "恢复笔记"
	}
	items := []*fyne.MenuItem{fyne.NewMenuItem("导出 Markdown", n.exportMarkdown), fyne.NewMenuItem("复制全文", func() {
		note := n.current()
		if note != nil {
			n.owner.Window.Clipboard().SetContent(note.Body)
		}
	}), fyne.NewMenuItem(text, n.toggleDeleted)}
	if note.Deleted {
		items = append(items, fyne.NewMenuItem("永久删除", func() {
			showConfirmDialog("永久删除笔记", "此操作无法恢复。建议先导出笔记，确认永久删除？", func(ok bool) {
				if ok {
					n.deleteCurrent()
				}
			}, n.owner.Window)
		}))
	}
	widget.NewPopUpMenu(fyne.NewMenu("", items...), n.owner.Window.Canvas()).ShowAtPosition(fyne.NewPos(max(0, n.owner.Window.Canvas().Size().Width-210), 48))
}
func (n *noteWorkspace) toggleDeleted() {
	note := n.current()
	if note == nil {
		return
	}
	note.Deleted = !note.Deleted
	note.UpdatedAt = time.Now().UTC()
	n.markDirty()
	n.selected = ""
	n.filter()
	n.showEmpty()
}
func (n *noteWorkspace) editGroups() {
	if !n.loaded {
		return
	}
	newNoteGroupsDialog(n).show()
}
func (n *noteWorkspace) addGroup(name string) error {
	candidate := n.book.Clone()
	candidate.Groups = append(candidate.Groups, domain.NoteGroup{ID: uuid.NewString(), Name: strings.TrimSpace(name)})
	if err := candidate.Validate(); err != nil {
		return err
	}
	n.book = candidate
	n.refreshGroups()
	n.markDirty()
	return nil
}
func (n *noteWorkspace) renameGroup(old, name string) error {
	candidate := n.book.Clone()
	for i := range candidate.Groups {
		if candidate.Groups[i].Name == old {
			candidate.Groups[i].Name = strings.TrimSpace(name)
			if err := candidate.Validate(); err != nil {
				return err
			}
			n.book = candidate
			n.refreshGroups()
			n.markDirty()
			return nil
		}
	}
	return domain.ErrNotFound
}
func (n *noteWorkspace) removeGroup(name string) {
	for i, g := range n.book.Groups {
		if g.Name == name {
			for j := range n.book.Notes {
				if n.book.Notes[j].GroupID == g.ID {
					n.book.Notes[j].GroupID = ""
				}
			}
			n.book.Groups = append(n.book.Groups[:i], n.book.Groups[i+1:]...)
			n.refreshGroups()
			n.filter()
			n.markDirty()
			if n.current() != nil {
				n.selectNote(n.selected)
			}
			return
		}
	}
}

func (n *noteWorkspace) deleteCurrent() {
	for i, note := range n.book.Notes {
		if note.ID == n.selected && note.Deleted {
			n.book.Notes = append(n.book.Notes[:i], n.book.Notes[i+1:]...)
			n.selected = ""
			n.markDirty()
			n.filter()
			n.showEmpty()
			return
		}
	}
}
