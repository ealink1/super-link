package ui

import (
	"slices"

	"fyne.io/fyne/v2/container"
)

// documents owns tab selection without laying out pages. The custom tab strip
// and body have a single layout owner; an off-screen DocTabs must not resize them.
type documents struct {
	Items                    []*container.TabItem
	selected                 *container.TabItem
	OnSelected, OnUnselected func(*container.TabItem)
	CreateTab                func() *container.TabItem
	CloseIntercept           func(*container.TabItem)
}

func (d *documents) Append(item *container.TabItem) {
	d.Items = append(d.Items, item)
	if d.selected == nil {
		d.Select(item)
	}
}
func (d *documents) Selected() *container.TabItem { return d.selected }
func (d *documents) SelectedIndex() int {
	return slices.Index(d.Items, d.selected)
}
func (d *documents) SelectIndex(index int) {
	if index >= 0 && index < len(d.Items) {
		d.Select(d.Items[index])
	}
}
func (d *documents) Select(item *container.TabItem) {
	if d.selected == item {
		return
	}
	for _, candidate := range d.Items {
		if candidate == item {
			old := d.selected
			d.selected = item
			if old != nil && d.OnUnselected != nil {
				d.OnUnselected(old)
			}
			if d.OnSelected != nil {
				d.OnSelected(item)
			}
			return
		}
	}
}
func (d *documents) Remove(item *container.TabItem) {
	for i, candidate := range d.Items {
		if candidate != item {
			continue
		}
		d.Items = slices.Delete(d.Items, i, i+1)
		if d.selected == item {
			d.selected = nil
			if d.OnUnselected != nil {
				d.OnUnselected(item)
			}
			if len(d.Items) > 0 {
				d.Select(d.Items[min(i, len(d.Items)-1)])
			} else if d.OnSelected != nil {
				d.OnSelected(nil)
			}
		}
		return
	}
}
func (d *documents) Refresh() {
	if d.OnSelected != nil {
		d.OnSelected(d.selected)
	}
}
