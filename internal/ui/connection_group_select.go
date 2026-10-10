package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

const connectionGroupMenuRowHeight float32 = 24

var connectionGroupMenuBlue = color.NRGBA{R: 0, G: 122, B: 255, A: 255}
var connectionGroupMenuGlass = shellTone{day: color.NRGBA{R: 252, G: 252, B: 254, A: 218}, night: color.NRGBA{R: 36, G: 38, B: 42, A: 235}}

// Keep Select's value and change semantics, with a compact native popover
// instead of the platform-neutral menu used by its default renderer.
type connectionGroupSelect struct {
	widget.Select
	canvas  fyne.Canvas
	popup   *widget.PopUp
	scope   *container.ThemeOverride
	list    *widget.List
	focused bool
	cursor  int
}

func newConnectionGroupSelect(canvas fyne.Canvas) *connectionGroupSelect {
	s := &connectionGroupSelect{canvas: canvas}
	s.ExtendBaseWidget(s)
	return s
}
func (s *connectionGroupSelect) CreateRenderer() fyne.WidgetRenderer {
	label := widget.NewLabel("")
	label.Wrapping = fyne.TextTruncate
	background := shellRectangle(shellPanelColor, 11, connectionGroupTableBorder)
	arrow := widget.NewIcon(shellIcon("chevron-down", false))
	body := shellBorder(nil, nil, shellFixed(layout.NewSpacer(), 8, 0), shellHBox(container.NewCenter(shellFixed(arrow, 16, 16)), shellFixed(layout.NewSpacer(), 14, 0)), label)
	r := &connectionGroupSelectRenderer{selectWidget: s, label: label, background: background, body: body}
	r.Refresh()
	return r
}
func (s *connectionGroupSelect) FocusGained() { s.focused = true; s.Refresh() }
func (s *connectionGroupSelect) FocusLost()   { s.focused = false; s.closePopup() }
func (s *connectionGroupSelect) Disable()     { s.closePopup(); s.Select.Disable() }
func (s *connectionGroupSelect) Tapped(*fyne.PointEvent) {
	if s.Disabled() {
		return
	}
	if s.popup != nil && s.popup.Visible() {
		s.closePopup()
		return
	}
	s.showPopup()
}
func (s *connectionGroupSelect) TypedKey(key *fyne.KeyEvent) {
	if s.Disabled() {
		return
	}
	switch key.Name {
	case fyne.KeyEscape:
		s.closePopup()
	case fyne.KeySpace, fyne.KeyReturn, fyne.KeyEnter:
		if s.popup != nil && s.popup.Visible() {
			s.choose(s.cursor)
		} else {
			s.showPopup()
		}
	case fyne.KeyUp, fyne.KeyDown:
		if s.popup == nil || !s.popup.Visible() {
			s.showPopup()
			return
		}
		if key.Name == fyne.KeyUp {
			s.cursor--
		} else {
			s.cursor++
		}
		s.cursor = max(0, min(len(s.Options)-1, s.cursor))
		s.list.Refresh()
		s.list.ScrollTo(widget.ListItemID(s.cursor))
	}
}
func (s *connectionGroupSelect) TypedRune(r rune) {
	if r == ' ' {
		s.TypedKey(&fyne.KeyEvent{Name: fyne.KeySpace})
	}
}
func (s *connectionGroupSelect) closePopup() {
	if s.popup != nil {
		s.popup.Hide()
	}
	s.Refresh()
}
func (s *connectionGroupSelect) choose(index int) {
	if s.Disabled() || index < 0 || index >= len(s.Options) {
		return
	}
	s.SetSelectedIndex(index)
	s.closePopup()
	s.canvas.Focus(s)
}
func (s *connectionGroupSelect) showPopup() {
	if s.Disabled() || len(s.Options) == 0 {
		return
	}
	s.closePopup()
	s.cursor = max(0, s.SelectedIndex())
	s.list = widget.NewList(func() int { return len(s.Options) }, func() fyne.CanvasObject { return newConnectionGroupSelectRow(s) }, func(index widget.ListItemID, object fyne.CanvasObject) {
		row := object.(*connectionGroupSelectRow)
		row.index = int(index)
		row.Refresh()
	})
	s.list.HideSeparators = true
	blur := canvas.NewBlur(18)
	blur.CornerRadius = 12
	panel := container.NewStack(blur, shellRectangle(connectionGroupMenuGlass, 12, connectionGroupTableBorder), shellInset(s.list, 4))
	s.popup = widget.NewPopUp(container.NewThemeOverride(panel, connectionGroupSelectTheme{connectionGroupFormTheme{newShellTheme()}}), s.canvas)
	s.scope = container.NewThemeOverride(s.popup, connectionGroupSelectTheme{connectionGroupFormTheme{newShellTheme()}})
	origin := fyne.CurrentApp().Driver().AbsolutePositionForObject(s)
	screen := s.canvas.Size()
	height := min(float32(len(s.Options))*connectionGroupMenuRowHeight+8, 200, screen.Height-16)
	// A native-style select aligns the current option over its control rather
	// than opening a separate panel beneath it. Leave the field's arrow visible.
	visibleRows := max(1, int((height-8)/connectionGroupMenuRowHeight))
	y := origin.Y - 4 - float32(min(s.cursor, visibleRows-1))*connectionGroupMenuRowHeight
	y = min(max(8, y), max(8, screen.Height-height-8))
	width := min(s.Size().Width, screen.Width-16)
	x := min(max(8, origin.X-16), max(8, screen.Width-width-8))
	s.popup.Resize(fyne.NewSize(width, height))
	s.popup.ShowAtPosition(fyne.NewPos(x, max(8, y)))
	s.list.ScrollTo(widget.ListItemID(s.cursor))
	s.canvas.Focus(s)
	s.Refresh()
}

type connectionGroupSelectRenderer struct {
	selectWidget *connectionGroupSelect
	label        *widget.Label
	background   *shellPrimitive
	body         *fyne.Container
}

func (*connectionGroupSelectRenderer) MinSize() fyne.Size { return fyne.NewSize(200, 44) }
func (r *connectionGroupSelectRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.body.Resize(size)
}
func (r *connectionGroupSelectRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.body}
}
func (*connectionGroupSelectRenderer) Destroy() {}
func (r *connectionGroupSelectRenderer) Refresh() {
	s := r.selectWidget
	text := s.Selected
	if text == "" {
		text = s.PlaceHolder
	}
	r.label.SetText(text)
	r.label.Importance = widget.MediumImportance
	r.background.stroke = connectionGroupTableBorder
	if s.focused && !s.Disabled() {
		r.background.stroke = connectionGroupPrimary
	}
	if s.Disabled() {
		r.label.Importance = widget.LowImportance
	}
	r.label.Refresh()
	r.background.Refresh()
	r.Layout(s.Size())
}

type connectionGroupSelectRow struct {
	widget.Button
	owner *connectionGroupSelect
	index int
}

func newConnectionGroupSelectRow(owner *connectionGroupSelect) *connectionGroupSelectRow {
	r := &connectionGroupSelectRow{owner: owner, index: -1}
	r.ExtendBaseWidget(r)
	r.OnTapped = func() { owner.choose(r.index) }
	return r
}
func (r *connectionGroupSelectRow) MouseIn(*desktop.MouseEvent) {
	if r.index >= 0 {
		r.owner.cursor = r.index
		r.owner.list.Refresh()
	}
}
func (*connectionGroupSelectRow) MouseOut() {}
func (r *connectionGroupSelectRow) CreateRenderer() fyne.WidgetRenderer {
	label := widget.NewLabel("")
	label.Wrapping = fyne.TextTruncate
	check := widget.NewIcon(groupFormIcon("check"))
	background := shellRectangle(color.Transparent, 6, nil)
	labelView := container.NewThemeOverride(label, connectionGroupSelectRowTheme{connectionGroupSelectTheme{connectionGroupFormTheme{newShellTheme()}}, r})
	body := shellBorder(nil, nil, shellHBox(shellFixed(layout.NewSpacer(), 6, 0), container.NewCenter(shellFixed(check, 14, 14)), shellFixed(layout.NewSpacer(), 6, 0)), shellFixed(layout.NewSpacer(), 8, 0), labelView)
	renderer := &connectionGroupSelectRowRenderer{row: r, label: label, check: check, background: background, body: body}
	renderer.Refresh()
	return renderer
}

type connectionGroupSelectRowRenderer struct {
	row        *connectionGroupSelectRow
	label      *widget.Label
	check      *widget.Icon
	background *shellPrimitive
	body       *fyne.Container
}

func (*connectionGroupSelectRowRenderer) MinSize() fyne.Size {
	return fyne.NewSize(200, connectionGroupMenuRowHeight)
}
func (r *connectionGroupSelectRowRenderer) Layout(size fyne.Size) {
	r.background.Resize(size)
	r.body.Resize(size)
}
func (r *connectionGroupSelectRowRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.background, r.body}
}
func (*connectionGroupSelectRowRenderer) Destroy() {}
func (r *connectionGroupSelectRowRenderer) Refresh() {
	row := r.row
	if row.index >= 0 && row.index < len(row.owner.Options) {
		r.label.SetText(row.owner.Options[row.index])
		if row.owner.Selected == row.owner.Options[row.index] {
			r.check.Show()
		} else {
			r.check.Hide()
		}
	}
	r.background.fill = color.Transparent
	if row.owner.cursor == row.index {
		r.background.fill = connectionGroupMenuBlue
		r.check.SetResource(groupFormIcon("check"))
	} else {
		resource := shellIcon("check", false)
		if outline, ok := resource.(*shellOutlineIcon); ok {
			outline.shade = theme.ColorNameForeground
		}
		r.check.SetResource(resource)
	}
	r.label.Refresh()
	r.background.Refresh()
	r.Layout(row.Size())
}

type connectionGroupSelectTheme struct{ connectionGroupFormTheme }

func (t connectionGroupSelectTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	switch name {
	case theme.ColorNameOverlayBackground:
		return color.Transparent
	case theme.ColorNameShadow:
		return color.NRGBA{R: 16, G: 24, B: 40, A: 36}
	}
	return t.connectionGroupFormTheme.Color(name, variant)
}
func (t connectionGroupSelectTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNamePopupRadius:
		return 12
	case theme.SizeNamePadding:
		return 0
	case theme.SizeNameInnerPadding:
		return 0
	}
	return t.connectionGroupFormTheme.Size(name)
}

type connectionGroupSelectRowTheme struct {
	connectionGroupSelectTheme
	row *connectionGroupSelectRow
}

func (t connectionGroupSelectRowTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground && t.row.owner.cursor == t.row.index {
		return color.White
	}
	return t.connectionGroupSelectTheme.Color(name, variant)
}
