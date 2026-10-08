package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

var monitorTrackColor = shellTone{day: color.NRGBA{187, 190, 194, 255}, night: color.NRGBA{72, 78, 87, 255}}
var monitorBlueColor = shellTone{day: color.NRGBA{43, 111, 255, 255}, night: color.NRGBA{99, 157, 255, 255}}

type monitorMetricCard struct {
	widget.BaseWidget
	title, value, left, right string
	fraction                  float64
	network                   bool
	shade                     color.Color
	action                    func()
}

func newMonitorMetricCard(title, value, left, right string, fraction float64, network bool, action func()) *monitorMetricCard {
	c := &monitorMetricCard{title: title, value: value, left: left, right: right, fraction: fraction, network: network, shade: monitorAccent(title), action: action}
	c.ExtendBaseWidget(c)
	return c
}
func (c *monitorMetricCard) Tapped(*fyne.PointEvent) {
	if c.action != nil {
		c.action()
	}
}
func (c *monitorMetricCard) CreateRenderer() fyne.WidgetRenderer {
	text := func(s string, size float32, bold, mono bool, align fyne.TextAlign, shade color.Color) fyne.CanvasObject {
		l := widget.NewLabel(s)
		l.TextStyle = fyne.TextStyle{Bold: bold, Monospace: mono}
		l.Alignment = align
		l.Truncation = fyne.TextTruncateEllipsis
		return container.NewThemeOverride(l, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: size}, shade: resolveShellColor(shade)})
	}
	leftShade, rightShade := color.Color(shellMutedColor), color.Color(shellMutedColor)
	size := float32(10)
	if c.network {
		leftShade, rightShade, size = c.shade, monitorBlueColor, 11
	}
	icon := canvas.NewImageFromResource(monitorMetricIcon(c.title, c.shade))
	icon.FillMode = canvas.ImageFillContain
	objects := []fyne.CanvasObject{shellRectangle(monitorIdentityFill, 10, shellBorderColor), icon, text(c.title, 11, true, false, fyne.TextAlignLeading, shellMutedColor), text(c.value, 15, false, true, fyne.TextAlignTrailing, c.shade), shellRectangle(monitorTrackColor, 3, nil), shellRectangle(c.shade, 3, nil), text(c.left, size, false, false, fyne.TextAlignLeading, leftShade), text(c.right, size, false, false, fyne.TextAlignTrailing, rightShade)}
	if c.network {
		objects[3].Hide()
		objects[4].Hide()
		objects[5].Hide()
	}
	return &monitorMetricRenderer{card: c, objects: objects}
}

type monitorMetricRenderer struct {
	card    *monitorMetricCard
	objects []fyne.CanvasObject
}

func (*monitorMetricRenderer) MinSize() fyne.Size { return fyne.NewSize(130, 88) }
func (r *monitorMetricRenderer) Layout(size fyne.Size) {
	o := r.objects
	place := func(i int, x, y, w, h float32) {
		o[i].Move(fyne.NewPos(x, y))
		if i == 2 || i == 3 || i == 6 || i == 7 {
			height := o[i].MinSize().Height
			textY := y + (h-height)/2
			if i == 2 && r.card.title != "CPU" {
				textY -= 3
			}
			o[i].Move(fyne.NewPos(x, textY))
			o[i].Resize(fyne.NewSize(max(0, w), height))
			return
		}
		o[i].Resize(fyne.NewSize(max(0, w), h))
	}
	place(0, 0, 0, size.Width, size.Height)
	place(1, 12, 17, 16, 16)
	place(2, 35, 14, 38, 22)
	place(3, 70, 10, size.Width-82, 30)
	width := max(0, size.Width-24)
	place(4, 12, 49, width, 6)
	place(5, 12, 49, width*float32(min(1, max(0, r.card.fraction))), 6)
	y := float32(64)
	if r.card.network {
		y = 44
	}
	place(6, 12, y, width/2, 22)
	place(7, 12+width/2, y, width/2, 22)
}
func (r *monitorMetricRenderer) Refresh()                     { r.Layout(r.card.Size()); canvas.Refresh(r.card) }
func (r *monitorMetricRenderer) Objects() []fyne.CanvasObject { return r.objects }
func (*monitorMetricRenderer) Destroy()                       {}
func monitorMetricIcon(title string, shade color.Color) fyne.Resource {
	paths := map[string]string{
		"CPU": `<rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M9 2v3m6-3v3M9 19v3m6-3v3M2 9h3m-3 6h3m14-6h3m-3 6h3"/>`,
		"内存":  `<rect x="3" y="3" width="18" height="7" rx="1"/><rect x="3" y="14" width="18" height="7" rx="1"/><path d="M6 6h.1M6 17h.1"/>`,
		"磁盘":  `<path d="M3 14 6 5h12l3 9"/><rect x="3" y="14" width="18" height="6" rx="1"/><path d="M6 17h.1m3-.1h.1"/>`,
		"网络":  `<path d="M2 8a16 16 0 0 1 20 0M5 12a11 11 0 0 1 14 0M9 16a5 5 0 0 1 6 0"/><circle cx="12" cy="20" r=".5"/>`,
	}
	paths["综合"] = `<path d="m12 3 10 5-10 5L2 8l10-5Zm-10 9 10 5 10-5M2 16l10 5 10-5"/>`
	paths["GPU"] = `<rect x="3" y="3" width="18" height="13" rx="1"/><path d="m10 7 4 3-4 3V7Zm2 9v5m-4 0h8"/>`
	paths["PATH"] = `<path d="M3 7V4h6l2 3h9v4M2 10h20l-3 10H3L2 10Z"/>`
	paths["DIR"] = `<path d="M3 20V4h6l2 3h10v13H3Z"/>`
	paths["FILE"] = `<path d="M5 2h9l5 5v15H5V2Zm9 0v6h5"/>`
	paths["TOTAL"] = paths["磁盘"]
	paths["展开"] = `<circle cx="12" cy="12" r="11"/><path d="m9 10 3-3 3 3m-6 4 3 3 3-3"/>`
	paths["趋势"] = `<path d="M2 12h5l3-9 4 18 3-9h5"/>`
	aliases := map[string]string{"MEM": "内存", "DISK": "磁盘", "NET": "网络"}
	if v, ok := aliases[title]; ok {
		title = v
	}
	c := color.NRGBAModel.Convert(resolveShellColor(shade)).(color.NRGBA)
	return fyne.NewStaticResource("monitor-"+title+".svg", []byte(fmt.Sprintf(`<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#%02x%02x%02x" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">%s</svg>`, c.R, c.G, c.B, paths[title])))
}
func (m *shellMonitor) overviewCards() fyne.CanvasObject {
	v := m.metrics
	cpu, left, right := m.cpuValue(), "User: —", "Sys: —"
	fraction := float64(0)
	if len(m.rates.CPUs) > 0 {
		c := m.rates.CPUs[0]
		left, right = fmt.Sprintf("User: %.1f%%", c.User), fmt.Sprintf("Sys: %.1f%%", c.System)
		fraction = c.Used / 100
	}
	memory := monitorRatio(v.MemoryUsed, v.MemoryTotal)
	_, _, _, disk := shellMetricDisplay(v)
	diskUsed, diskTotal := "—", "—"
	for _, p := range v.Details.Partitions {
		if p.Mount == "/" {
			diskUsed, diskTotal = fmt.Sprintf("%.0f GB", float64(p.Used)/(1<<30)), fmt.Sprintf("%.0f GB", float64(p.Total)/(1<<30))
			disk = p.Percent / 100
			break
		}
	}
	down, up := "↓ —", "↑ —"
	if m.rates.Ready {
		a, b := monitorHostNetworkTotals(m.rates.Networks)
		down, up = "↓ "+monitorSpeed(a), "↑ "+monitorSpeed(b)
	}
	return container.New(layout.NewCustomPaddedVBoxLayout(12), shellColumns(12, newMonitorMetricCard("CPU", cpu, left, right, fraction, false, m.selectTab(1)), newMonitorMetricCard("内存", fmt.Sprintf("%.0f%%", memory*100), fmt.Sprintf("%.1f GB", float64(v.MemoryUsed)/(1<<30)), fmt.Sprintf("%.1f GB", float64(v.MemoryTotal)/(1<<30)), memory, false, m.selectTab(3))), shellColumns(12, newMonitorMetricCard("磁盘", fmt.Sprintf("%.0f%%", disk*100), diskUsed, diskTotal, disk, false, m.selectTab(4)), newMonitorMetricCard("网络", "", down, up, 0, true, m.selectTab(5))))
}
func monitorSpeed(n float64) string {
	if n >= 1<<20 {
		return fmt.Sprintf("%.1fMB/s", n/(1<<20))
	}
	if n >= 1024 {
		return fmt.Sprintf("%.0fKB/s", n/1024)
	}
	return fmt.Sprintf("%.0fB/s", n)
}
