package ui

import (
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (n *noteWorkspace) build() {
	n.status = widget.NewLabel("正在读取本地笔记…")
	n.status.Truncation = fyne.TextTruncateEllipsis
	n.count = widget.NewLabel("")
	n.search = widget.NewEntry()
	n.search.SetPlaceHolder("搜索笔记…")
	n.search.SetIcon(shellIcon("search", false))
	n.search.OnChanged = func(string) { n.filter() }
	n.newButton = shellButton("新建笔记", "plus", true, n.newNote)
	n.newButton.Disable()
	n.saveButton = shellButton("保存", "save", false, n.save)
	n.saveButton.Disable()
	n.buildSidebarTree()
	top := shellInset(shellVBox(shellFixed(n.search, 0, 40), shellFixed(layout.NewSpacer(), 0, 12), shellFixed(shellBorder(nil, nil, nil, shellButtonView(shellButton("", "folder", false, n.editGroups)), container.NewThemeOverride(n.newButton, noteSidebarTheme{newShellTheme()})), 0, 36), shellFixed(layout.NewSpacer(), 0, 16), shellFixed(shellBorder(nil, nil, shellLabel(n.sidebarHeading, 12), shellLabel(n.sidebarCount, 11), layout.NewSpacer()), 0, 28)), 16)
	footer := shellInset(shellHBox(shellButtonView(shellButton("回收站", "trash-2", false, func() { n.trash = !n.trash; n.selected = ""; n.filter(); n.showEmpty() })), layout.NewSpacer(), shellButtonView(shellButton("导入", "upload", false, n.importMarkdown))), 10)
	sidebar := container.NewThemeOverride(container.NewStack(shellRectangle(noteSidebarSurface, 0, nil), shellBorder(top, footer, nil, nil, container.NewThemeOverride(n.list, noteTreeTheme{noteSidebarTheme{newShellTheme()}}))), noteSidebarTheme{newShellTheme()})
	n.body = container.NewStack()
	n.buildEditor()
	n.showEmpty()
	n.content = container.NewThemeOverride(container.NewStack(shellRectangle(noteSidebarSurface, 0, nil), container.New(&noteWorkspaceLayout{}, sidebar, n.body)), noteSidebarTheme{newShellTheme()})
}

type noteWorkspaceLayout struct{}

func (*noteWorkspaceLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(700, 460) }
func (*noteWorkspaceLayout) Layout(o []fyne.CanvasObject, s fyne.Size) {
	width := min(280, max(232, s.Width*.28))
	o[0].Move(fyne.Position{})
	o[0].Resize(fyne.NewSize(width, s.Height))
	o[1].Move(fyne.NewPos(width+1, 0))
	o[1].Resize(fyne.NewSize(max(0, s.Width-width-1), s.Height))
}

type noteEntryTheme struct {
	shellTheme
	size float32
}

func (t noteEntryTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return t.size
	}
	if name == theme.SizeNameInnerPadding {
		return 0
	}
	return t.shellTheme.Size(name)
}
func (t noteEntryTheme) Color(name fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameInputBackground {
		return color.Transparent
	}
	return t.shellTheme.Color(name, v)
}

func (n *noteWorkspace) buildEditor() {
	n.title = newNoteEntry(false)
	n.title.SetPlaceHolder("无标题笔记")
	n.title.TextStyle.Bold = true
	n.editor = newNoteEntry(true)
	n.editor.rich = true
	n.editor.SetPlaceHolder("开始记录…")
	n.editor.Wrapping = fyne.TextWrapOff
	n.editor.Scroll = container.ScrollNone
	n.tags = widget.NewEntry()
	n.tags.SetPlaceHolder("标签，逗号分隔")
	n.title.OnChanged = func(string) { n.changed(); n.list.Refresh() }
	n.editor.OnChanged = func(string) { n.editor.recordEdit(); n.changed(); n.refreshFormatState() }
	n.editor.OnCursorChanged = n.refreshFormatState
	n.tags.OnChanged = func(string) { n.changed() }
	n.groupPicker = widget.NewSelect([]string{"默认分组"}, func(name string) {
		if n.binding {
			return
		}
		note := n.current()
		if note == nil || note.Deleted {
			return
		}
		note.GroupID = ""
		for _, g := range n.book.Groups {
			if g.Name == name {
				note.GroupID = g.ID
			}
		}
		n.changed()
		n.activeGroup = note.GroupID
		n.filter()
		n.list.OpenBranch(noteGroupNode(note.GroupID))
	})
	n.preview = widget.NewRichText()
	n.preview.Wrapping = fyne.TextWrapWord
	n.viewPicker = widget.NewSelect([]string{"编辑", "源码", "预览", "分屏"}, func(value string) { n.view = value; n.updateEditorView() })
	n.viewPicker.SetSelected("编辑")
	header := shellFixed(shellInset(shellBorder(nil, nil, nil, shellHBox(shellFixed(n.groupPicker, 130, 28), shellButtonView(shellButton("更多", "ellipsis", false, n.showMore))), shellLabel(n.status, 13)), 24), 0, 64)
	title := shellFixed(noteTitleInset(container.NewThemeOverride(n.title, noteEntryTheme{shellTheme: newShellTheme(), size: 28})), 0, 80)
	format := container.NewHScroll(n.formatToolbar())
	format.SetMinSize(fyne.NewSize(280, 36))
	toolbar := shellFixed(shellInset(shellBorder(nil, nil, nil, shellFixed(n.viewPicker, 92, 28), format), 12), 0, 56)
	bottom := shellFixed(shellInset(shellBorder(nil, nil, shellLabel(n.count, 11), noteButtonView(n.saveButton), shellFixed(n.tags, 0, 28)), 12), 0, 52)
	n.editorHost = shellBorder(shellVBox(header, shellLine(), title, noteHorizontalInset(shellVBox(toolbar, shellLine()), 48)), shellVBox(shellLine(), bottom), nil, nil, n.editorArea())
}
func (n *noteWorkspace) editorArea() fyne.CanvasObject {
	return container.NewVScroll(shellBorder(shellFixed(layout.NewSpacer(), 0, 24), shellFixed(layout.NewSpacer(), 0, 24), shellFixed(layout.NewSpacer(), 48, 0), shellFixed(layout.NewSpacer(), 48, 0), container.NewThemeOverride(n.editor, noteEntryTheme{shellTheme: newShellTheme(), size: 16})))
}
func (n *noteWorkspace) updateEditorView() {
	if n.editorHost == nil {
		return
	}
	n.editor.rich = n.view != "源码" && n.view != "分屏"
	n.editor.TextStyle.Monospace = !n.editor.rich
	n.editor.Refresh()
	n.renderPreview()
	var body fyne.CanvasObject = n.editorArea()
	if n.view == "预览" {
		body = container.NewVScroll(shellInset(n.preview, 28))
	} else if n.view == "分屏" {
		split := container.NewHSplit(n.editorArea(), container.NewVScroll(shellInset(n.preview, 28)))
		split.Offset = .5
		body = split
	}
	// shellBorder places its center first; fixed regions stay attached.
	n.editorHost.Objects[0] = body
	n.editorHost.Refresh()
}

type noteRow struct {
	widget.BaseWidget
	title, summary, stamp *widget.Label
	fill                  *shellPrimitive
	line                  fyne.CanvasObject
	accent                *shellPrimitive
	selected              bool
}

func newNoteRow() *noteRow {
	r := &noteRow{title: widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), summary: widget.NewLabel(""), stamp: widget.NewLabel(""), fill: shellRectangle(color.Transparent, 0, nil), line: shellLine(), accent: shellRectangle(noteSidebarAccent, 0, nil)}
	for _, l := range []*widget.Label{r.title, r.summary, r.stamp} {
		l.Truncation = fyne.TextTruncateEllipsis
	}
	r.ExtendBaseWidget(r)
	return r
}
func (r *noteRow) update(note domain.Note, selected bool) {
	title := strings.TrimSpace(note.Title)
	if title == "" {
		title = "无标题笔记"
	}
	r.title.SetText(title)
	summary := noteSidebarSummary(note.Body)
	if summary == "" {
		summary = "空白笔记"
	}
	r.summary.SetText(truncateShellTitle(summary, 120))
	r.stamp.SetText(note.UpdatedAt.Local().Format("01/02 15:04"))
	r.selected = selected
	r.accent.Hide()
	if selected {
		r.accent.Show()
	}
	r.fill.fill = color.Transparent
	if selected {
		r.fill.fill = resolveShellColor(noteSidebarSelection)
	}
	r.line.Show()
	r.fill.Refresh()
}
func (r *noteRow) CreateRenderer() fyne.WidgetRenderer {
	return &noteRowRenderer{objects: []fyne.CanvasObject{r.fill, container.NewThemeOverride(r.title, noteRowTheme{shellLabelTheme: shellLabelTheme{newShellTheme(), 14}, row: r, title: true}), container.NewThemeOverride(r.summary, noteRowTheme{shellLabelTheme: shellLabelTheme{newShellTheme(), 12}, row: r}), container.NewThemeOverride(r.stamp, noteRowTheme{shellLabelTheme: shellLabelTheme{newShellTheme(), 11}, row: r}), r.line, r.accent}}
}

type noteRowRenderer struct{ objects []fyne.CanvasObject }

func (*noteRowRenderer) MinSize() fyne.Size { return fyne.NewSize(220, 90) }
func (r *noteRowRenderer) Layout(s fyne.Size) {
	r.objects[0].Move(fyne.Position{})
	r.objects[0].Resize(s)
	for i, o := range r.objects[1:4] {
		o.Move(fyne.NewPos(16, 10+float32(i)*23))
		o.Resize(fyne.NewSize(max(0, s.Width-28), 22))
	}
	r.objects[4].Move(fyne.NewPos(0, s.Height-1))
	r.objects[4].Resize(fyne.NewSize(s.Width, 1))
	r.objects[5].Move(fyne.Position{})
	r.objects[5].Resize(fyne.NewSize(2, s.Height))
}
func (r *noteRowRenderer) Objects() []fyne.CanvasObject { return r.objects }
func (*noteRowRenderer) Destroy()                       {}
func (r *noteRowRenderer) Refresh() {
	for _, o := range r.objects {
		o.Refresh()
	}
}

// Horizontal title padding follows the reference, while vertical padding keeps
// the 26px title font within the entry's full line height.
func noteTitleInset(object fyne.CanvasObject) fyne.CanvasObject {
	return shellBorder(shellFixed(layout.NewSpacer(), 0, 12), shellFixed(layout.NewSpacer(), 0, 12), shellFixed(layout.NewSpacer(), 48, 0), shellFixed(layout.NewSpacer(), 48, 0), object)
}

func noteHorizontalInset(object fyne.CanvasObject, padding float32) fyne.CanvasObject {
	return shellBorder(nil, nil, shellFixed(layout.NewSpacer(), padding, 0), shellFixed(layout.NewSpacer(), padding, 0), object)
}
