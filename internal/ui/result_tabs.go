package ui

import (
	"image/color"
	"strconv"
	"strings"
	"unicode"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// resultTabs gives the output bar SuperLink's compact rounded tabs while retaining
// native keyboard-focusable buttons. Only the selected page owns a layout.
type resultTabs struct {
	widget.BaseWidget
	Items    []*container.TabItem
	selected int
	bottom   bool
	counts   map[*container.TabItem]int
	buttons  map[*container.TabItem]resultTabButton
	head     *fyne.Container
	body     *fyne.Container
}

type resultTabButton struct {
	button  *shellAlignedButton
	content fyne.CanvasObject
	state   *resultButtonState
}

// Themes can outlive their rendered children. Keep selection separate from the
// tabs, whose contents may include a large query result.
type resultButtonState struct{ selected bool }

func newResultTabs(items ...*container.TabItem) *resultTabs {
	t := &resultTabs{Items: items, counts: map[*container.TabItem]int{}, buttons: map[*container.TabItem]resultTabButton{}}
	t.ExtendBaseWidget(t)
	return t
}
func (t *resultTabs) SelectedIndex() int {
	if len(t.Items) == 0 {
		return -1
	}
	return t.selected
}
func (t *resultTabs) Selected() *container.TabItem {
	if index := t.SelectedIndex(); index >= 0 {
		return t.Items[index]
	}
	return nil
}
func (t *resultTabs) Select(item *container.TabItem) {
	for i, candidate := range t.Items {
		if candidate == item {
			t.SelectIndex(i)
			return
		}
	}
}
func (t *resultTabs) SelectIndex(index int) {
	if index < 0 || index >= len(t.Items) {
		return
	}
	t.selected = index
	t.Refresh()
}
func (t *resultTabs) SetItems(items []*container.TabItem) {
	selected := t.Selected()
	t.Items = items
	t.selected = 0
	for i, item := range items {
		if item == selected {
			t.selected = i
		}
	}
	clear(t.counts)
	t.Refresh()
}
func (t *resultTabs) setCount(item *container.TabItem, count int) {
	t.counts[item] = count
	t.Refresh()
}
func (t *resultTabs) CreateRenderer() fyne.WidgetRenderer {
	t.head, t.body = container.NewHBox(), container.NewStack()
	t.sync()
	scroll := container.NewHScroll(t.head)
	scroll.SetMinSize(fyne.NewSize(0, 34))
	if t.bottom {
		return widget.NewSimpleRenderer(container.NewBorder(nil, scroll, nil, nil, t.body))
	}
	return widget.NewSimpleRenderer(container.NewBorder(scroll, nil, nil, nil, t.body))
}
func (t *resultTabs) Refresh() {
	t.sync()
	t.BaseWidget.Refresh()
}
func (t *resultTabs) sync() {
	if t.head == nil {
		return
	}
	buttons := make([]fyne.CanvasObject, 0, len(t.Items))
	active := make(map[*container.TabItem]bool, len(t.Items))
	for _, item := range t.Items {
		active[item] = true
		label := item.Text
		if count, ok := t.counts[item]; ok {
			label += "  " + strconv.Itoa(count)
		}
		cached, ok := t.buttons[item]
		if !ok {
			button := &shellAlignedButton{}
			button.ExtendBaseWidget(button)
			button.Text, button.OnTapped = label, func() { t.Select(item) }
			state := &resultButtonState{selected: item == t.Selected()}
			cached = resultTabButton{button: button, state: state, content: container.NewThemeOverride(button, resultButtonTheme{state: state})}
			t.buttons[item] = cached
		}
		cached.state.selected = item == t.Selected()
		// Latin capitals occupy a shorter, higher ink box than Chinese glyphs.
		cached.button.textOffset = 3
		if strings.ContainsFunc(label, func(r rune) bool { return unicode.Is(unicode.Han, r) }) {
			cached.button.textOffset = 0
		}
		cached.button.SetText(label)
		buttons = append(buttons, cached.content)
	}
	for item := range t.buttons {
		if !active[item] {
			t.buttons[item].button.OnTapped = nil
			delete(t.buttons, item)
		}
	}
	buttons = append(buttons, layout.NewSpacer())
	t.head.Objects = buttons
	t.head.Refresh()
	if item := t.Selected(); item != nil {
		t.body.Objects = []fyne.CanvasObject{item.Content}
	} else {
		t.body.Objects = nil
	}
	t.body.Refresh()
}

type resultButtonTheme struct {
	state *resultButtonState
}

func (t resultButtonTheme) Font(style fyne.TextStyle) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Font(style)
}
func (t resultButtonTheme) Icon(name fyne.ThemeIconName) fyne.Resource {
	return fyne.CurrentApp().Settings().Theme().Icon(name)
}
func (t resultButtonTheme) Size(name fyne.ThemeSizeName) float32 {
	return fyne.CurrentApp().Settings().Theme().Size(name)
}
func (t resultButtonTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	current := fyne.CurrentApp().Settings().Theme()
	if t.state.selected {
		if name == theme.ColorNameButton {
			if value, ok := current.(Theme); ok && value.Dark {
				return color.NRGBA{R: 24, G: 63, B: 44, A: 255}
			}
			return color.NRGBA{R: 232, G: 244, B: 235, A: 255}
		}
		if name == theme.ColorNameForeground {
			return current.Color(theme.ColorNamePrimary, variant)
		}
	}
	return current.Color(name, variant)
}
