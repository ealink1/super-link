package ui

import (
	"context"
	"fmt"
	"net"
	"slices"
	"sort"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
)

type connectionGroups struct {
	tableHost              *fyne.Container
	owner                  *Window
	popup                  *widget.PopUp
	scope                  *container.ThemeOverride
	profiles, rows         []domain.Profile
	groups                 []string
	parents                map[string]string
	options                map[string]domain.ConnectionGroupOptions
	active                 string
	selected               map[string]bool
	sidebar                *fyne.Container
	title, feedback, empty *widget.Label
	total, count           *widget.Label
	search                 *widget.Entry
	statuses               map[string]application.ConnectionStatus
	order                  *shellFormSelect
	destination            *widget.Select
	move                   *shellAlignedButton
	table                  *widget.Table
	busy                   bool
}

type connectionGroupSnapshot struct {
	profiles []domain.Profile
	groups   []string
	parents  map[string]string
	options  map[string]domain.ConnectionGroupOptions
	statuses map[string]application.ConnectionStatus
}

func (w *Window) groupManager() { newConnectionGroups(w).show() }

func newConnectionGroups(w *Window) *connectionGroups {
	d := &connectionGroups{owner: w, selected: map[string]bool{}}
	d.title = widget.NewLabelWithStyle("未分组", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	d.feedback = widget.NewLabel("正在读取连接…")
	d.empty = widget.NewLabel("此分组暂无连接")
	d.empty.Alignment = fyne.TextAlignCenter
	d.sidebar = shellVBox()
	d.total = widget.NewLabel("")
	d.count = widget.NewLabel("")
	d.search = widget.NewEntry()
	d.search.SetPlaceHolder("搜索连接名称 / 地址")
	d.search.OnChanged = func(string) { d.filter() }
	d.order = newShellFormSelect([]string{"添加时间 ↓", "添加时间 ↑", "连接名称 ↑", "连接名称 ↓", "地址 ↑"})
	d.order.OnChanged = func(string) { d.filter() }
	d.order.SetSelectedIndex(0)
	d.destination = widget.NewSelect(nil, func(string) { d.updateMove() })
	d.destination.PlaceHolder = "选择目标分组"
	d.move = shellButton("移动所选连接", "", false, d.moveSelected)
	d.move.Disable()
	d.table = d.newTable()
	d.build()
	return d
}

func (d *connectionGroups) show() { d.popup.Show(); d.load() }

func (d *connectionGroups) load() {
	d.busy = true
	d.updateMove()
	d.owner.jobs.run(func(ctx context.Context) (any, error) { return readConnectionGroupSnapshot(ctx, d.owner) }, func(value any, err error) {
		d.busy = false
		if err != nil {
			d.feedback.SetText(err.Error())
			return
		}
		snapshot := value.(connectionGroupSnapshot)
		d.profiles = snapshot.profiles
		d.groups = snapshot.groups
		d.parents = snapshot.parents
		d.options = snapshot.options
		d.statuses = snapshot.statuses
		d.total.SetText(fmt.Sprintf("共 %d 个连接", len(d.profiles)))
		for _, p := range d.profiles {
			if p.Group != "" && !slices.Contains(d.groups, p.Group) {
				d.groups = append(d.groups, p.Group)
			}
		}
		d.selected = map[string]bool{}
		d.rebuildSidebar()
		d.filter()
	})
}

func (d *connectionGroups) rebuildSidebar() {
	d.sidebar.Objects = nil
	names := append([]string{""}, connectionGroupOrder(d.groups, d.parents)...)
	for _, name := range names {
		count := 0
		for _, p := range d.profiles {
			if p.Group == name {
				count++
			}
		}
		d.sidebar.Add(d.groupItem(name, count))
		d.sidebar.Add(noteGroupsGap(4))
	}
	d.destination.Options = nil
	for _, name := range names {
		if name != d.active {
			d.destination.Options = append(d.destination.Options, groupLabel(name))
		}
	}
	d.destination.ClearSelected()
	d.destination.Refresh()
}

func groupLabel(name string) string {
	if name == "" {
		return "未分组"
	}
	return name
}
func connectionAddress(p domain.Profile) string {
	if p.Config.Host == "" {
		return "—"
	}
	if p.Config.Port <= 0 {
		return p.Config.Host
	}
	return net.JoinHostPort(p.Config.Host, strconv.Itoa(p.Config.Port))
}

func (d *connectionGroups) filter() {
	if d.table == nil {
		return
	}
	d.rows = nil
	for _, p := range d.profiles {
		if p.Group == d.active && strings.Contains(strings.ToLower(p.Name+" "+connectionAddress(p)), strings.ToLower(strings.TrimSpace(d.search.Text))) {
			d.rows = append(d.rows, p)
		}
	}
	sort.SliceStable(d.rows, func(i, j int) bool {
		a, b := d.rows[i], d.rows[j]
		switch d.order.Selected {
		case "添加时间 ↑":
			return a.CreatedAt.Before(b.CreatedAt)
		case "连接名称 ↑":
			return a.Name < b.Name
		case "连接名称 ↓":
			return a.Name > b.Name
		case "地址 ↑":
			return connectionAddress(a) < connectionAddress(b)
		default:
			return a.CreatedAt.After(b.CreatedAt)
		}
	})
	d.title.SetText(groupLabel(d.active))
	d.count.SetText(fmt.Sprintf("· %d 个连接", len(d.rows)))
	d.empty.Hide()
	if len(d.rows) == 0 {
		d.empty.SetText("此分组暂无连接")
		if strings.TrimSpace(d.search.Text) != "" {
			d.empty.SetText("未找到匹配的连接")
		}
		d.empty.Show()
	}
	d.tableHost.Refresh()
	d.table.Refresh()
	d.updateMove()
}

func (d *connectionGroups) updateMove() {
	d.move.Disable()
	count := 0
	for _, p := range d.profiles {
		if p.Group == d.active && d.selected[p.ID] {
			count++
		}
	}
	if count > 0 && d.destination.Selected != "" && !d.busy {
		d.move.Enable()
	}
	if !d.busy {
		d.feedback.SetText(fmt.Sprintf("%d 个连接 · 已选择 %d 个", len(d.rows), count))
	}
}

func (d *connectionGroups) createGroup() { newConnectionGroupForm(d).show() }

func (d *connectionGroups) moveSelected() {
	if d.busy {
		return
	}
	group := d.destination.Selected
	if group == "" {
		return
	}
	if group == "未分组" {
		group = ""
	}
	var chosen []domain.Profile
	for _, p := range d.profiles {
		if p.Group == d.active && d.selected[p.ID] {
			chosen = append(chosen, p)
		}
	}
	if len(chosen) == 0 {
		return
	}
	d.busy = true
	d.updateMove()
	d.feedback.SetText("正在移动连接…")
	d.owner.jobs.run(func(ctx context.Context) (any, error) { return nil, d.owner.Profiles.MoveToGroup(ctx, chosen, group) }, func(_ any, err error) {
		d.busy = false
		if err != nil {
			d.owner.showError(err)
			d.load()
			return
		}
		d.active = group
		d.owner.reload()
		d.load()
	})
}

func (d *connectionGroups) deleteConnection(p domain.Profile) {
	showConfirmDialog("删除连接", "删除「"+p.Name+"」及其本地草稿和历史？", func(ok bool) {
		if !ok {
			return
		}
		d.owner.cancelProfile(p.ID)
		d.owner.jobs.run(func(ctx context.Context) (any, error) {
			if err := d.owner.Engine.Disconnect(ctx, p.ID); err != nil {
				return nil, err
			}
			return nil, d.owner.Profiles.Delete(ctx, p.ID, p.Revision)
		}, func(_ any, err error) {
			if err != nil {
				d.owner.showError(err)
				return
			}
			d.owner.removeProfileDocuments(p.ID)
			if d.owner.selected == p.ID {
				d.owner.selected = ""
			}
			d.owner.reload()
			d.load()
		})
	}, d.owner.Window)
}
