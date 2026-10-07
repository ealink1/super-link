package ui

import (
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
	"strings"
)

var sourceCategories = []string{"全部", "关系型数据库", "国产数据库", "NoSQL 数据库", "向量数据库", "时序数据库", "消息队列", "配置中心", "其他"}

func sourceCategory(d domain.Descriptor) string {
	switch d.Key {
	case "goldendb", "dameng", "kingbase", "highgo", "vastbase", "opengauss", "gaussdb", "oceanbase":
		return "国产数据库"
	case "tdengine", "iotdb":
		return "时序数据库"
	case "custom":
		return "其他"
	}
	switch d.Family {
	case domain.SQL:
		return "关系型数据库"
	case domain.Cache, domain.Document, domain.Search:
		return "NoSQL 数据库"
	case domain.Vector:
		return "向量数据库"
	case domain.Message:
		return "消息队列"
	case domain.Configuration:
		return "配置中心"
	}
	return "其他"
}

func (w *Window) typePicker() {
	var modal *widget.PopUp
	picker := newSourcePicker(w.dark, w.Version, func(d domain.Descriptor) {
		modal.Hide()
		w.editProfile(domain.Profile{ReadOnly: true, Config: connection.ConnectionConfig{Type: d.Key, Host: "localhost", Port: d.Port, User: "root", Timeout: 30, QueryTimeout: 30}})
	}, func() { modal.Hide() })
	modal = widget.NewModalPopUp(picker.content, w.Window.Canvas())
	size := w.Window.Canvas().Size()
	modal.Resize(fyne.NewSize(min(1180, max(560, size.Width-48)), min(780, max(420, size.Height-64))))
	modal.Show()
}

type sourcePicker struct {
	content                  *container.ThemeOverride
	search                   *widget.Entry
	nav                      *widget.List
	cards                    *fyne.Container
	scroll                   *container.Scroll
	selected, count, summary *canvas.Text
	empty                    *fyne.Container
	category                 string
	colors                   sourcePickerTheme
	choose                   func(domain.Descriptor)
}

func newSourcePicker(dark bool, version string, choose func(domain.Descriptor), close func()) *sourcePicker {
	p := &sourcePicker{category: "全部", colors: sourcePickerTheme{Theme: Theme{Dark: dark}}, choose: choose}
	p.search = widget.NewEntry()
	p.search.Icon = theme.SearchIcon()
	p.search.SetPlaceHolder("搜索数据源类型")
	p.cards = container.New(&sourceGridLayout{})
	p.scroll = container.NewVScroll(p.cards)
	p.selected = p.text("全部", 16, true, "text")
	p.count = p.text("", 13, false, "muted")
	p.summary = p.text("", 12, false, "muted")
	counts := map[string]int{}
	for _, d := range domain.Catalog() {
		counts[sourceCategory(d)]++
		counts["全部"]++
	}
	p.nav = widget.NewList(func() int { return len(sourceCategories) }, func() fyne.CanvasObject {
		label := widget.NewLabel("")
		label.TextStyle.Bold = true
		count := widget.NewLabel("")
		count.Importance = widget.LowImportance
		return container.New(layout.NewCustomPaddedLayout(5, 5, 8, 8), container.NewBorder(nil, nil, nil, count, label))
	}, func(id int, obj fyne.CanvasObject) {
		row := obj.(*fyne.Container).Objects[0].(*fyne.Container)
		label, count := row.Objects[0].(*widget.Label), row.Objects[1].(*widget.Label)
		label.Importance, count.Importance = widget.MediumImportance, widget.LowImportance
		if sourceCategories[id] == p.category {
			label.Importance, count.Importance = widget.HighImportance, widget.HighImportance
		}
		label.SetText(sourceCategories[id])
		count.SetText(fmt.Sprint(counts[sourceCategories[id]]))
	})
	p.nav.HideSeparators = true
	p.nav.OnSelected = func(id int) { p.category = sourceCategories[id]; p.nav.Refresh(); p.refresh() }
	p.search.OnChanged = func(string) { p.refresh() }
	left := p.panel(container.NewBorder(container.NewVBox(p.search, p.spacer(8)), nil, nil, nil, p.nav), 14)
	title := container.NewHBox(p.selected, p.spacer(4), p.count)
	heading := container.NewBorder(nil, nil, title, nil, container.NewHBox(layout.NewSpacer(), p.text("单击进入配置表单", 12, false, "muted")))
	footer := container.NewVBox(p.spacer(10), widget.NewSeparator(), p.spacer(6), container.NewBorder(nil, nil, nil, p.text("SuperLink v"+version, 12, false, "muted"), p.summary))
	p.empty = container.NewCenter(container.NewVBox(p.text("没有匹配的数据源", 16, true, "text"), p.text("试试其他名称，或切换左侧分类", 13, false, "muted")))
	p.empty.Hide()
	main := p.panel(container.NewBorder(container.NewVBox(heading, p.spacer(12)), footer, nil, nil, container.NewStack(p.scroll, p.empty)), 20)
	body := container.New(&sourceColumnsLayout{}, left, main)
	closeButton := widget.NewButtonWithIcon("", theme.CancelIcon(), close)
	closeButton.Importance = widget.LowImportance
	logo := canvas.NewRectangle(p.colors.shade("brand"))
	logo.CornerRadius = 8
	mark := canvas.NewText("SL", hexColor("#ffffff"))
	mark.TextSize = 12
	mark.TextStyle.Bold = true
	mark.Alignment = fyne.TextAlignCenter
	brand := container.NewHBox(container.NewGridWrap(fyne.NewSquareSize(28), container.NewStack(logo, container.NewCenter(mark))), p.spacer(4), p.text("SuperLink", 16, true, "text"))
	steps := container.NewHBox(p.step("1  选类型", true), p.text("—", 12, false, "line"), p.step("2  配参数", false), p.text("—", 12, false, "line"), p.step("3  测试保存", false))
	header := p.panel(container.NewBorder(nil, nil, brand, closeButton, container.NewHBox(layout.NewSpacer(), steps)), 14)
	background := canvas.NewRectangle(p.colors.shade("background"))
	view := container.NewStack(background, container.New(layout.NewCustomPaddedLayout(16, 16, 16, 16), container.NewBorder(container.NewVBox(header, p.spacer(12)), nil, nil, nil, body)))
	p.content = container.NewThemeOverride(view, p.colors)
	p.nav.Select(0)
	return p
}

func (p *sourcePicker) refresh() {
	p.cards.Objects = nil
	needle := strings.ToLower(strings.TrimSpace(p.search.Text))
	for _, d := range domain.Catalog() {
		if p.category != "全部" && sourceCategory(d) != p.category || !strings.Contains(strings.ToLower(d.Name+" "+d.Key), needle) {
			continue
		}
		p.cards.Add(newSourceCard(d, p.colors, func() { p.choose(d) }))
	}
	count := len(p.cards.Objects)
	p.selected.Text = p.category
	p.selected.Refresh()
	p.count.Text = fmt.Sprintf("%d 个数据源", count)
	p.count.Refresh()
	p.summary.Text = fmt.Sprintf("共 %d 个数据源 · 当前显示 %d 个", len(domain.Catalog()), count)
	p.summary.Refresh()
	if count == 0 {
		p.empty.Show()
		p.scroll.Hide()
	} else {
		p.empty.Hide()
		p.scroll.Show()
	}
	p.cards.Refresh()
	p.scroll.ScrollToTop()
	if p.content != nil {
		p.content.Refresh()
	}
}

func (p *sourcePicker) text(text string, size float32, bold bool, shade string) *canvas.Text {
	result := canvas.NewText(text, p.colors.shade(shade))
	result.TextSize = size
	result.TextStyle.Bold = bold
	return result
}
func (p *sourcePicker) spacer(size float32) fyne.CanvasObject {
	return container.NewGridWrap(fyne.NewSize(size, size), layout.NewSpacer())
}
func (p *sourcePicker) panel(content fyne.CanvasObject, padding float32) fyne.CanvasObject {
	bg := canvas.NewRectangle(p.colors.shade("panel"))
	bg.CornerRadius = 14
	bg.StrokeColor = p.colors.shade("line")
	bg.StrokeWidth = 1
	return container.NewStack(bg, container.New(layout.NewCustomPaddedLayout(padding, padding, padding, padding), content))
}
func (p *sourcePicker) step(text string, active bool) fyne.CanvasObject {
	label := p.text(text, 13, active, "muted")
	if !active {
		return container.New(layout.NewCustomPaddedLayout(6, 6, 12, 12), label)
	}
	label.Color = p.colors.shade("brand")
	bg := canvas.NewRectangle(p.colors.shade("soft"))
	bg.CornerRadius = 14
	return container.NewStack(bg, container.New(layout.NewCustomPaddedLayout(6, 6, 12, 12), label))
}
