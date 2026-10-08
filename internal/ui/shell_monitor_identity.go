package ui

import (
	"image/color"
	"math"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

var monitorIdentityFill = shellTone{day: color.NRGBA{233, 235, 239, 255}, night: color.NRGBA{34, 39, 46, 255}}
var monitorBadgeFill = shellTone{day: color.NRGBA{222, 225, 230, 255}, night: color.NRGBA{45, 51, 60, 255}}
var monitorIPColor = shellTone{day: color.NRGBA{0, 149, 163, 255}, night: color.NRGBA{40, 199, 210, 255}}
var monitorUserColor = shellTone{day: color.NRGBA{229, 82, 188, 255}, night: color.NRGBA{244, 112, 210, 255}}
var monitorUptimeColor = shellTone{day: color.NRGBA{49, 157, 36, 255}, night: color.NRGBA{103, 205, 85, 255}}

func (m *shellMonitor) identityCard() fyne.CanvasObject {
	name, address, user := "SSH 主机", "—", "—"
	if h := m.pane.host; h != nil {
		name, address, user = h.Name, h.Host, h.User
	}
	label := func(text string, size float32, bold, mono bool, shade color.Color) fyne.CanvasObject {
		l := widget.NewLabel(text)
		l.TextStyle = fyne.TextStyle{Bold: bold, Monospace: mono}
		l.Truncation = fyne.TextTruncateEllipsis
		return container.NewThemeOverride(l, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: size}, shade: resolveShellColor(shade)})
	}
	ip := newMonitorIdentityBadge(address, monitorIPColor, func() { m.pane.workspace.owner.Window.Clipboard().SetContent(address) })
	username := newMonitorIdentityBadge(user, monitorUserColor, nil)
	kernel := m.metrics.Details.Kernel
	if kernel == "" {
		kernel = "等待读取系统信息"
	}
	objects := []fyne.CanvasObject{shellRectangle(monitorIdentityFill, 12, shellBorderColor), shellImage("server", true, 18), label(name, 13, true, false, shellTextColor), ip, username, label("· "+kernel, 11, false, true, shellMutedColor), label("运行时间", 10, false, false, shellMutedColor), label(monitorUptime(m.metrics.Uptime), 13, true, true, monitorUptimeColor)}
	for _, i := range []int{6, 7} {
		objects[i].(*container.ThemeOverride).Content.(*widget.Label).Alignment = fyne.TextAlignTrailing
	}
	return container.New(&monitorIdentityLayout{}, objects...)
}
func monitorUptime(raw string) string {
	f := strings.Fields(raw)
	if len(f) == 0 {
		return "—"
	}
	n, err := strconv.ParseFloat(f[0], 64)
	if err != nil || math.IsNaN(n) || math.IsInf(n, 0) || n < 0 || n > 1e12 {
		return "—"
	}
	hours, minutes := int64(n)/3600, int64(n)%3600/60
	return strconv.FormatInt(hours, 10) + "h " + strconv.FormatInt(minutes, 10) + "m"
}

type monitorIdentityLayout struct{}

func (*monitorIdentityLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(280, 70) }
func (*monitorIdentityLayout) Layout(o []fyne.CanvasObject, size fyne.Size) {
	place := func(i int, x, y, w, h float32) { o[i].Move(fyne.NewPos(x, y)); o[i].Resize(fyne.NewSize(max(0, w), h)) }
	place(0, 0, 0, size.Width, size.Height)
	place(1, 14, (size.Height-18)/2, 18, 18)
	rightWidth := float32(76)
	x := float32(40)
	available := max(0, size.Width-x-rightWidth-24)
	ipWidth := min(o[3].MinSize().Width, max(60, available-36))
	name := o[2].(*container.ThemeOverride).Content.(*widget.Label).Text
	nameWidth := min(fyne.MeasureText(name, 13, fyne.TextStyle{Bold: true}).Width+2, max(24, available-ipWidth-8))
	place(2, x, 13, nameWidth, 24)
	place(3, x+nameWidth+8, 12, ipWidth, 25)
	userWidth := min(o[4].MinSize().Width, max(44, available*0.4))
	place(4, x, 38, userWidth, 24)
	place(5, x+userWidth+8, 38, max(0, available-userWidth-8), 24)
	place(6, size.Width-rightWidth-14, 14, rightWidth, 18)
	place(7, size.Width-rightWidth-14, 33, rightWidth, 25)
}

type monitorIdentityBadge struct {
	widget.BaseWidget
	text  string
	shade color.Color
	copy  func()
}

func newMonitorIdentityBadge(text string, shade color.Color, copy func()) *monitorIdentityBadge {
	b := &monitorIdentityBadge{text: text, shade: shade, copy: copy}
	b.ExtendBaseWidget(b)
	return b
}
func (b *monitorIdentityBadge) Tapped(*fyne.PointEvent) {
	if b.copy != nil {
		b.copy()
	}
}
func (b *monitorIdentityBadge) CreateRenderer() fyne.WidgetRenderer {
	text := widget.NewLabel(b.text)
	text.TextStyle.Monospace = true
	text.Truncation = fyne.TextTruncateEllipsis
	content := container.NewThemeOverride(text, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 11}, shade: resolveShellColor(b.shade)})
	return widget.NewSimpleRenderer(container.NewStack(shellRectangle(monitorBadgeFill, 6, nil), shellInset(content, 4)))
}

func (b *monitorIdentityBadge) MinSize() fyne.Size {
	return fyne.NewSize(fyne.MeasureText(b.text, 11, fyne.TextStyle{Monospace: true}).Width+12, 25)
}
