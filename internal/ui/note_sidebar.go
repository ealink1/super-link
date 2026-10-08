package ui

import (
	"fmt"
	"sort"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func noteGroupNode(id string) string { return "group:" + id }
func noteLeafNode(id string) string  { return "note:" + id }

func (n *noteWorkspace) buildSidebarTree() {
	n.groupNotes, n.collapsed = make(map[string][]string), make(map[string]bool)
	n.sidebarHeading, n.sidebarCount = widget.NewLabel("笔记分组"), widget.NewLabel("0 篇")
	n.list = widget.NewTree(func(id string) []string {
		if id == "" {
			return n.groupNodes
		}
		return n.groupNotes[id]
	}, func(id string) bool { return id == "" || strings.HasPrefix(id, "group:") }, func(branch bool) fyne.CanvasObject {
		if branch {
			return newNoteGroupRow()
		}
		return newNoteRow()
	}, func(id string, branch bool, object fyne.CanvasObject) {
		if branch {
			row := object.(*noteGroupRow)
			if n.list.IsBranchOpen(id) {
				row.arrow.SetResource(shellIcon("chevron-down", false))
			} else {
				row.arrow.SetResource(shellIcon("chevron-right", false))
			}
			row.title.SetText(n.groupName(strings.TrimPrefix(id, "group:")))
			row.count.SetText(fmt.Sprint(len(n.groupNotes[id])))
			row.Refresh()
			return
		}
		noteID := strings.TrimPrefix(id, "note:")
		object.(*noteRow).update(n.noteByID(noteID), noteID == n.selected)
	})
	n.list.OnSelected = func(id string) {
		if strings.HasPrefix(id, "group:") {
			n.activeGroup = strings.TrimPrefix(id, "group:")
			n.list.ToggleBranch(id)
			n.list.UnselectAll()
			return
		}
		n.selectNote(strings.TrimPrefix(id, "note:"))
	}
	n.list.OnBranchClosed = func(id string) {
		if !n.treeRefreshing {
			n.collapsed[id] = true
		}
	}
	n.list.OnBranchOpened = func(id string) {
		if !n.treeRefreshing {
			delete(n.collapsed, id)
		}
	}
}

type noteGroupRow struct {
	widget.BaseWidget
	title, count *widget.Label
	arrow        *widget.Icon
}

func newNoteGroupRow() *noteGroupRow {
	title := widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	title.Truncation = fyne.TextTruncateEllipsis
	row := &noteGroupRow{title: title, count: widget.NewLabel(""), arrow: widget.NewIcon(shellIcon("chevron-down", false))}
	row.ExtendBaseWidget(row)
	return row
}

func (r *noteGroupRow) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(container.New(&noteGroupRowLayout{}, r.arrow, widget.NewIcon(theme.FolderIcon()), shellLabel(r.title, 12), shellLabel(r.count, 11)))
}

type noteGroupRowLayout struct{}

func (*noteGroupRowLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(172, max(40, objects[0].MinSize().Height, objects[1].MinSize().Height))
}

func (*noteGroupRowLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Move(fyne.NewPos(12, (size.Height-12)/2))
	objects[0].Resize(fyne.NewSize(12, 12))
	objects[1].Move(fyne.NewPos(32, (size.Height-14)/2))
	objects[1].Resize(fyne.NewSize(14, 14))
	countWidth := float32(28)
	for i, object := range objects[2:] {
		height := object.MinSize().Height
		x, width := float32(54), max(0, size.Width-countWidth-54)
		if i == 1 {
			x, width = max(0, size.Width-countWidth), countWidth
		}
		object.Move(fyne.NewPos(x, (size.Height-height)/2))
		object.Resize(fyne.NewSize(width, height))
	}
}

func (n *noteWorkspace) filter() {
	if n.list == nil {
		return
	}
	n.visible = nil
	needle := strings.ToLower(strings.TrimSpace(n.search.Text))
	for _, note := range n.book.Notes {
		if note.Deleted != n.trash {
			continue
		}
		text := note.Title + "\n" + note.Body + "\n" + note.Tags + "\n" + n.groupName(note.GroupID)
		if strings.Contains(strings.ToLower(text), needle) {
			n.visible = append(n.visible, note.ID)
		}
	}
	sort.SliceStable(n.visible, func(i, j int) bool {
		a, b := n.noteByID(n.visible[i]), n.noteByID(n.visible[j])
		if a.UpdatedAt.Equal(b.UpdatedAt) {
			return a.ID < b.ID
		}
		return a.UpdatedAt.After(b.UpdatedAt)
	})
	n.groupNotes = make(map[string][]string, len(n.book.Groups)+1)
	for _, id := range n.visible {
		key := noteGroupNode(n.noteByID(id).GroupID)
		n.groupNotes[key] = append(n.groupNotes[key], noteLeafNode(id))
	}
	n.groupNodes = nil
	groups := append([]domain.NoteGroup{{Name: "默认分组"}}, n.book.Groups...)
	valid := make(map[string]bool, len(groups))
	for _, group := range groups {
		key := noteGroupNode(group.ID)
		valid[key] = true
		if (needle != "" || n.trash) && len(n.groupNotes[key]) == 0 && (n.trash || !strings.Contains(strings.ToLower(group.Name), needle)) {
			continue
		}
		n.groupNodes = append(n.groupNodes, key)
	}
	for key := range n.collapsed {
		if !valid[key] {
			delete(n.collapsed, key)
		}
	}
	if !valid[noteGroupNode(n.activeGroup)] {
		n.activeGroup = ""
	}
	n.treeRefreshing = true
	n.list.UnselectAll()
	n.list.Refresh()
	for _, key := range n.groupNodes {
		if needle != "" || !n.collapsed[key] {
			n.list.OpenBranch(key)
		} else {
			n.list.CloseBranch(key)
		}
	}
	n.treeRefreshing = false
	n.sidebarHeading.SetText("笔记分组")
	if n.trash {
		n.sidebarHeading.SetText("回收站")
	}
	n.sidebarCount.SetText(fmt.Sprintf("%d 篇", len(n.visible)))
	if n.current() == nil {
		n.showEmpty()
	}
}
