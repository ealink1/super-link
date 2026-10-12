package ui

import (
	"slices"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
)

type tabCloseGroup int

const (
	tabsLeft tabCloseGroup = iota
	tabsRight
	tabsOthers
	tabsAll
)

func (d *documents) closeMenu(anchor *container.TabItem, close func(tabCloseGroup)) *fyne.Menu {
	labels := []string{"关闭左边标签", "关闭右边标签", "关闭其他标签", "关闭所有标签"}
	items := make([]*fyne.MenuItem, 0, len(labels))
	for group, label := range labels {
		group := tabCloseGroup(group)
		item := fyne.NewMenuItem(label, func() { close(group) })
		item.Disabled = len(d.tabsToClose(anchor, group)) == 0
		items = append(items, item)
	}
	return fyne.NewMenu("", items...)
}

func (d *documents) tabsToClose(anchor *container.TabItem, group tabCloseGroup) []*container.TabItem {
	index := slices.Index(d.Items, anchor)
	if index < 0 {
		return nil // Stale menus must not close surviving or newly opened tabs.
	}
	switch group {
	case tabsLeft:
		return slices.Clone(d.Items[:index])
	case tabsRight:
		return slices.Clone(d.Items[index+1:])
	case tabsOthers:
		items := slices.Clone(d.Items[:index])
		return append(items, d.Items[index+1:]...)
	case tabsAll:
		return slices.Clone(d.Items)
	default:
		return nil
	}
}
