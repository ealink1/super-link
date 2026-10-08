package ui

import (
	"fmt"
	"image/color"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"

	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

var monitorSurfaceColor = shellTone{day: color.NRGBA{238, 240, 244, 255}, night: color.NRGBA{28, 33, 40, 255}}
var monitorCoral = sharedUIColor(theme.ColorNamePrimary)
var monitorSelectedFill = sharedUIColor(theme.ColorNameSelection)
var monitorMutedColor = shellTone{day: color.NRGBA{124, 126, 128, 255}, night: color.NRGBA{170, 175, 182, 255}}

func (m *shellMonitor) navigationView() fyne.CanvasObject {
	objects := []fyne.CanvasObject{}
	for i, name := range []string{"综合", "CPU", "GPU", "MEM", "DISK", "NET"} {
		index := i
		shade := color.Color(monitorMutedColor)
		fill := color.Color(color.Transparent)
		if i == m.tabs.SelectedIndex() {
			shade, fill = monitorCoral, monitorSelectedFill
		}
		button := newMonitorTabButton(name, monitorMetricIcon(name, shade), func() { m.tabs.SelectIndex(index) })
		object := container.NewThemeOverride(button, monitorNavigationTheme{monitorTintTheme: monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 10}, shade: resolveShellColor(shade)}})
		objects = append(objects, container.NewStack(shellRectangle(fill, 4, nil), object))
	}
	objects = append(objects, shellButtonView(shellButton("", "panel-left-close", false, m.pane.hideAuxiliary)))
	strip := &monitorNavigation{monitor: m, content: container.New(&monitorNavigationLayout{}, objects...)}
	strip.ExtendBaseWidget(strip)
	return strip
}

type monitorNavigation struct {
	widget.BaseWidget
	monitor *shellMonitor
	content fyne.CanvasObject
}

func (n *monitorNavigation) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(n.content)
}
func (n *monitorNavigation) TappedSecondary(e *fyne.PointEvent) {
	m := n.monitor
	autoText := "暂停自动采样"
	if !m.auto.Checked {
		autoText = "恢复自动采样"
	}
	items := []*fyne.MenuItem{fyne.NewMenuItem("刷新", m.sample), fyne.NewMenuItem(autoText, func() { m.auto.SetChecked(!m.auto.Checked) }), fyne.NewMenuItemSeparator()}
	for _, seconds := range []int{1, 3, 5, 10} {
		duration := time.Duration(seconds) * time.Second
		item := fyne.NewMenuItem(fmt.Sprintf("每 %d 秒采样", seconds), func() { m.interval = duration })
		item.Checked = m.interval == duration
		items = append(items, item)
	}
	widget.NewPopUpMenu(fyne.NewMenu("监控", items...), m.pane.workspace.owner.Window.Canvas()).ShowAtPosition(e.AbsolutePosition)
}

var _ fyne.SecondaryTappable = (*monitorNavigation)(nil)

type monitorNavigationLayout struct{}

func (*monitorNavigationLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(280, 40) }
func (*monitorNavigationLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	width := max(0, (size.Width-48)/6)
	y := max(0, (size.Height-28)/2)
	for i := 0; i < 6; i++ {
		objects[i].Move(fyne.NewPos(8+float32(i)*width, y))
		objects[i].Resize(fyne.NewSize(width, 28))
	}
	objects[6].Move(fyne.NewPos(size.Width-32, y))
	objects[6].Resize(fyne.NewSize(24, 28))
}
func (m *shellMonitor) footerView() fyne.CanvasObject {
	directory := m.summaryDirectory
	if directory == "" {
		directory = "—"
	}
	values := []string{directory, fmt.Sprint(m.summaryDirs), fmt.Sprint(m.summaryFiles), fmt.Sprintf("%.2f MB", float64(m.summaryBytes)/(1<<20))}
	shades := []color.Color{color.NRGBA{140, 129, 255, 255}, color.NRGBA{140, 129, 255, 255}, monitorMutedColor, color.NRGBA{0, 220, 145, 255}}
	objects := []fyne.CanvasObject{}
	for i, title := range []string{"PATH", "DIR", "FILE", "TOTAL"} {
		objects = append(objects, newMonitorFileStat(title, values[i], shades[i], m.pane.showFiles))
	}
	stats := container.New(&monitorStatsLayout{}, objects...)
	expandButton := shellButton("", "", false, m.pane.showFiles)
	expandButton.SetIcon(monitorMetricIcon("展开", monitorMutedColor))
	expand := shellButtonView(expandButton)
	// The small handle sits above the four statistics without adding a toolbar.
	return shellInset(shellVBox(shellFixed(container.NewCenter(shellFixed(expand, 20, 16)), 0, 16), shellFixed(stats, 0, 46)), 16)
}

type monitorStatsLayout struct{}

func (*monitorStatsLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(280, 46) }
func (*monitorStatsLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	width := (size.Width - 24) / 4
	for i, o := range objects {
		o.Move(fyne.NewPos(float32(i)*(width+8), 0))
		o.Resize(fyne.NewSize(width, size.Height))
	}
}

type monitorFileStat struct {
	widget.BaseWidget
	title, value string
	shade        color.Color
	action       func()
}

func newMonitorFileStat(title, value string, shade color.Color, action func()) *monitorFileStat {
	s := &monitorFileStat{title: title, value: value, shade: shade, action: action}
	s.ExtendBaseWidget(s)
	return s
}
func (s *monitorFileStat) Tapped(*fyne.PointEvent) { s.action() }
func (s *monitorFileStat) CreateRenderer() fyne.WidgetRenderer {
	icon := canvas.NewImageFromResource(monitorMetricIcon(s.title, s.shade))
	icon.FillMode = canvas.ImageFillContain
	title := widget.NewLabel(s.title)
	value := widget.NewLabel(s.value)
	value.Truncation = fyne.TextTruncateEllipsis
	value.TextStyle.Monospace = true
	value.Alignment = fyne.TextAlignTrailing
	shade := color.Color(shellTextColor)
	if s.title == "TOTAL" {
		shade = color.NRGBA{61, 224, 165, 255}
	}
	if s.title == "PATH" {
		value.Alignment = fyne.TextAlignLeading
	}
	label := container.NewThemeOverride(value, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 10}, shade: shade})
	head := shellHBox(shellFixed(icon, 14, 14), shellFixed(layout.NewSpacer(), 4, 0), container.NewThemeOverride(title, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 10}, shade: monitorMutedColor}))
	return widget.NewSimpleRenderer(container.NewStack(shellRectangle(monitorIdentityFill, 8, shellBorderColor), shellInset(shellVBox(head, label), 10)))
}

type monitorFooterLayout struct{}

func (*monitorFooterLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(280, 84) }
func (*monitorFooterLayout) Layout(o []fyne.CanvasObject, size fyne.Size) {
	o[0].Move(fyne.NewPos((size.Width-20)/2, size.Height-70))
	o[0].Resize(fyne.NewSize(20, 16))
	o[1].Move(fyne.NewPos(16, size.Height-54))
	o[1].Resize(fyne.NewSize(size.Width-32, 46))
}

// Compact labels share a regular font and small icons, while retaining the
// aligned native button renderer and the monitoring palette.
type monitorNavigationTheme struct{ monitorTintTheme }

func (t monitorNavigationTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameInlineIcon {
		return 14
	}
	return t.monitorTintTheme.Size(name)
}
func (t monitorNavigationTheme) Font(style fyne.TextStyle) fyne.Resource {
	style.Bold = false
	return t.monitorTintTheme.Font(style)
}
