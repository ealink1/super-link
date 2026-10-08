package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

// Virtualize rows of cards rather than retaining a widget for every saved host.
type shellHostCollection struct {
	widget.BaseWidget
	workspace *shellWorkspace
	list      *widget.List
	empty     fyne.CanvasObject
	columns   int
}

func newShellHostCollection(s *shellWorkspace) *shellHostCollection {
	h := &shellHostCollection{workspace: s, columns: 3}
	h.ExtendBaseWidget(h)
	h.list = widget.NewList(func() int { return (len(s.visible) + h.columns - 1) / h.columns }, func() fyne.CanvasObject {
		cards := []fyne.CanvasObject{newShellHostCard(s), newShellHostCard(s), newShellHostCard(s)}
		height := float32(260)
		if s.listMode {
			height = 140
		}
		grid := shellColumns(16, cards...)
		return shellFixed(shellRowInset(grid), 0, height)
	}, func(row widget.ListItemID, object fyne.CanvasObject) {
		frame := object.(*fyne.Container)
		grid := frame.Objects[0].(*fyne.Container).Objects[0].(*fyne.Container)
		grid.Layout.(*shellColumnsLayout).slots = h.columns
		frame.Layout.(*shellFixedLayout).height = 260
		if s.listMode {
			frame.Layout.(*shellFixedLayout).height = 140
		}
		for i, child := range grid.Objects {
			card := child.(*shellHostCard)
			index := row*h.columns + i
			if i >= h.columns || index >= len(s.visible) {
				card.Hide()
				continue
			}
			card.bind(s.visible[index], s.listMode)
			card.Show()
		}
	})
	h.list.HideSeparators = true
	h.empty = container.NewCenter(shellVBox(shellImage("server", false, 48), shellFixed(layout.NewSpacer(), 0, 18), shellText("暂无主机", 15, true, shellTextColor), shellText("新建主机，或调整搜索与筛选条件", 12, false, shellMutedColor)))
	return h
}

func (h *shellHostCollection) CreateRenderer() fyne.WidgetRenderer {
	return &shellHostCollectionRenderer{hosts: h}
}

type shellHostCollectionRenderer struct{ hosts *shellHostCollection }

func (*shellHostCollectionRenderer) MinSize() fyne.Size { return fyne.NewSize(300, 260) }
func (r *shellHostCollectionRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.hosts.list, r.hosts.empty}
}
func (*shellHostCollectionRenderer) Destroy() {}
func (r *shellHostCollectionRenderer) Layout(size fyne.Size) {
	h := r.hosts
	columns := min(3, max(1, int(size.Width/316)))
	if h.workspace.listMode {
		columns = 1
	}
	changed := h.columns != columns
	h.columns = columns
	h.list.Resize(size)
	h.empty.Resize(size)
	if changed {
		h.list.Refresh()
	}
}
func (r *shellHostCollectionRenderer) Refresh() {
	r.Layout(r.hosts.Size())
	h := r.hosts
	h.list.Refresh()
	if len(h.workspace.visible) == 0 {
		h.list.Hide()
		h.empty.Show()
	} else {
		h.empty.Hide()
		h.list.Show()
	}
}

type shellHostCard struct {
	widget.BaseWidget
	workspace                  *shellWorkspace
	host                       domain.ShellHost
	name, address, notes, tags *widget.Label
	connect, menu              *shellAlignedButton
	content                    fyne.CanvasObject
	bound, listMode            bool
}

func newShellHostCard(s *shellWorkspace) *shellHostCard {
	c := &shellHostCard{workspace: s, name: widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), address: widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Monospace: true}), notes: widget.NewLabel(""), tags: widget.NewLabel("")}
	for _, label := range []*widget.Label{c.name, c.address, c.notes, c.tags} {
		label.Truncation = fyne.TextTruncateEllipsis
	}
	c.address.Importance, c.notes.Importance, c.tags.Importance = widget.LowImportance, widget.LowImportance, widget.LowImportance
	c.menu = shellButton("", "ellipsis", false, func() { s.hostActions(c.host) })
	c.connect = shellButton("快速连接", "terminal", false, func() { s.connectHost(c.host) })
	c.ExtendBaseWidget(c)
	return c
}

func (c *shellHostCard) bind(host domain.ShellHost, list bool) {
	c.host = host
	c.name.SetText(host.Name)
	address := fmt.Sprintf("%s@%s:%d", host.User, host.Host, host.Port)
	if c.workspace.privacy {
		address = "••••@••••••"
	}
	c.address.SetText(address)
	notes := strings.Split(host.Notes, "\n")[0]
	if notes == "" {
		notes = "暂无备注"
	}
	c.notes.SetText(notes)
	tags := host.Tags
	if tags == "" {
		tags = "无标签"
	}
	c.tags.SetText(tags)
	if c.bound && c.listMode == list {
		c.Refresh()
		return
	}
	c.bound, c.listMode = true, list
	top := shellBorder(nil, nil, shellPanel(shellImage("terminal", false, 24), shellRailColor, 8, 8), c.menu, layout.NewSpacer())
	protocol := shellProtocolBadge()
	details := shellVBox(container.New(&shellNameLayout{text: func() string { return c.name.Text }}, shellLabel(c.name, 14), protocol), shellFixed(layout.NewSpacer(), 0, 6), shellLabel(c.address, 12), shellFixed(layout.NewSpacer(), 0, 8), shellLabel(c.notes, 11))
	if list {
		c.content = shellPanel(shellBorder(nil, nil, top, shellFixed(shellOutlined(c.connect), 120, 30), shellInset(details, 8)), shellPanelColor, 10, 12)
	} else {
		footer := shellVBox(shellLabel(c.tags, 11), shellFixed(layout.NewSpacer(), 0, 16), shellFixed(shellOutlined(c.connect), 0, 30))
		c.content = shellPanel(shellBorder(top, footer, nil, nil, shellVBox(shellFixed(layout.NewSpacer(), 0, 12), details)), shellPanelColor, 12, 16)
	}
	c.Refresh()
}

func (c *shellHostCard) CreateRenderer() fyne.WidgetRenderer {
	if c.content == nil {
		c.content = layout.NewSpacer()
	}
	return &shellHostCardRenderer{card: c}
}

type shellHostCardRenderer struct{ card *shellHostCard }

func (*shellHostCardRenderer) MinSize() fyne.Size      { return fyne.NewSize(200, 80) }
func (r *shellHostCardRenderer) Layout(size fyne.Size) { r.card.content.Resize(size) }
func (r *shellHostCardRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.card.content}
}
func (r *shellHostCardRenderer) Refresh() { r.card.content.Refresh(); r.Layout(r.card.Size()) }
func (*shellHostCardRenderer) Destroy()   {}
