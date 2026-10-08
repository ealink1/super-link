package ui

import (
	"context"
	"fmt"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/google/uuid"
)

type noteWorkspace struct {
	formatButtons                           []noteFormatButton
	owner                                   *Window
	service                                 *application.Notebook
	book                                    domain.Notebook
	content                                 *container.ThemeOverride
	body, editorHost                        *fyne.Container
	search, tags                            *widget.Entry
	title, editor                           *noteEntry
	groupPicker, viewPicker                 *widget.Select
	list                                    *widget.Tree
	groupNodes                              []string
	groupNotes                              map[string][]string
	collapsed                               map[string]bool
	activeGroup                             string
	treeRefreshing                          bool
	sidebarHeading, sidebarCount            *widget.Label
	preview                                 *widget.RichText
	status, count                           *widget.Label
	saveButton, newButton                   *shellAlignedButton
	visible                                 []string
	selected                                string
	trash, loading, loaded, binding, saving bool
	view                                    string
	editRevision, savedRevision             uint64
	edits                                   chan struct{}
}

func newNoteWorkspace(w *Window) *noteWorkspace {
	n := &noteWorkspace{owner: w, service: &application.Notebook{Store: w.Store, Vault: w.Profiles.Vault}, loading: true, view: "编辑"}
	n.build()
	n.startAutosave()
	n.load()
	return n
}

func (n *noteWorkspace) load() {
	n.owner.jobs.run(func(ctx context.Context) (any, error) { return n.service.Load(ctx) }, func(value any, err error) {
		n.loading = false
		if err != nil {
			n.status.SetText("加载失败，原有笔记未被修改：" + err.Error())
			return
		}
		n.book = value.(domain.Notebook)
		n.loaded = true
		n.newButton.Enable()
		n.refreshGroups()
		n.filter()
		n.status.SetText("本地笔记已加载")
		for _, id := range n.visible {
			n.selectNote(id)
			break
		}
	})
}

func (n *noteWorkspace) current() *domain.Note {
	for i := range n.book.Notes {
		if n.book.Notes[i].ID == n.selected {
			return &n.book.Notes[i]
		}
	}
	return nil
}
func (n *noteWorkspace) newNote() {
	if !n.loaded || n.owner.shuttingDown {
		return
	}
	if len(n.book.Notes) >= domain.MaxNotes {
		n.status.SetText("笔记数量达到 300 篇，请先导出整理")
		return
	}
	group := n.filterGroupID()
	n.trash = false
	n.book.Notes = append(n.book.Notes, domain.Note{ID: uuid.NewString(), Title: "无标题笔记", GroupID: group, UpdatedAt: time.Now().UTC()})
	n.search.SetText("")
	n.markDirty()
	n.refreshGroups()
	n.filter()
	n.selectNote(n.book.Notes[len(n.book.Notes)-1].ID)
	n.owner.Window.Canvas().Focus(n.title)
}
func (n *noteWorkspace) selectNote(id string) {
	n.selected = id
	note := n.current()
	if note == nil {
		n.showEmpty()
		return
	}
	n.activeGroup = note.GroupID
	n.list.OpenBranch(noteGroupNode(note.GroupID))
	n.binding = true
	n.title.SetText(note.Title)
	n.editor.SetText(note.Body)
	n.tags.SetText(note.Tags)
	n.groupPicker.SetSelected(n.groupName(note.GroupID))
	n.binding = false
	if note.Deleted {
		n.title.Disable()
		n.editor.Disable()
		n.tags.Disable()
		n.groupPicker.Disable()
	} else {
		n.title.Enable()
		n.editor.Enable()
		n.tags.Enable()
		n.groupPicker.Enable()
	}
	n.body.Objects = []fyne.CanvasObject{n.editorHost}
	n.body.Refresh()
	n.renderPreview()
	n.updateCount()
	n.list.Refresh()
}
func (n *noteWorkspace) showEmpty() {
	message := "选择或新建一篇笔记"
	if n.trash {
		message = "回收站为空；移到回收站的笔记可以恢复"
	}
	n.body.Objects = []fyne.CanvasObject{container.NewCenter(shellVBox(shellImage("notebook-pen", true, 40), shellFixed(shellText(message, 16, false, shellMutedColor), 360, 64)))}
	n.body.Refresh()
}
func (n *noteWorkspace) changed() {
	if n.binding || n.owner.shuttingDown {
		return
	}
	note := n.current()
	if note == nil || note.Deleted {
		return
	}
	note.Title, note.Body, note.Tags = n.title.Text, n.editor.Text, n.tags.Text
	note.UpdatedAt = time.Now().UTC()
	n.updateCount()
	n.markDirty()
}
func (n *noteWorkspace) markDirty() {
	n.editRevision++
	n.status.SetText("未保存 · 自动保存中…")
	n.saveButton.Enable()
	select {
	case n.edits <- struct{}{}:
	default:
	}
}
func (n *noteWorkspace) noteByID(id string) domain.Note {
	for _, note := range n.book.Notes {
		if note.ID == id {
			return note
		}
	}
	return domain.Note{}
}
func (n *noteWorkspace) groupName(id string) string {
	for _, g := range n.book.Groups {
		if g.ID == id {
			return g.Name
		}
	}
	return "默认分组"
}
func (n *noteWorkspace) filterGroupID() string {
	for _, group := range n.book.Groups {
		if group.ID == n.activeGroup {
			return group.ID
		}
	}
	return ""
}
func (n *noteWorkspace) updateCount() {
	n.count.SetText(fmt.Sprintf("%d 字", len([]rune(n.editor.Text))))
}
