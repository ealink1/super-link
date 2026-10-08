package ui

import (
	"fmt"
	"image/color"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

func monitorSection(title, icon string, shade color.Color) fyne.CanvasObject {
	image := canvas.NewImageFromResource(monitorMetricIcon(icon, shade))
	image.FillMode = canvas.ImageFillContain
	label := widget.NewLabel(title)
	label.TextStyle.Bold = true
	return container.New(&monitorSectionLayout{}, image, container.NewThemeOverride(label, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 11}, shade: monitorMutedColor}))
}
func (m *shellMonitor) overviewPartitions() fyne.CanvasObject {
	content := container.New(layout.NewCustomPaddedVBoxLayout(8), monitorSection("磁盘分区", "磁盘", monitorAccent("磁盘")))
	for _, p := range m.metrics.Details.Partitions {
		content.Add(newMonitorDetailRow(p.Mount, fmt.Sprintf("%.0f%%", p.Percent), "", p.Percent/100, false))
	}
	return content
}
func (m *shellMonitor) overviewNetworks() fyne.CanvasObject {
	content := container.New(layout.NewCustomPaddedVBoxLayout(8), monitorSection("网络接口", "网络", monitorAccent("网络")))
	for _, n := range m.metrics.Details.Networks {
		if !monitorOverviewInterface(n.Name) {
			continue
		}
		down, up := "↓ —", "↑ —"
		for _, r := range m.rates.Networks {
			if r.Name == n.Name {
				down, up = "↓ "+monitorSpeed(r.Received), "↑ "+monitorSpeed(r.Sent)
				break
			}
		}
		content.Add(newMonitorDetailRow(n.Name, down, up, 0, true))
	}
	return content
}
func (m *shellMonitor) overviewChart() fyne.CanvasObject {
	chart := newTotalMonitorChart(m.history)
	ticks := container.New(&monitorTicksLayout{})
	count := 8
	for i := 0; i < count; i++ {
		text := ""
		if len(m.history) > 0 {
			first, last := m.history[0].Time, m.history[len(m.history)-1].Time
			text = first.Add(last.Sub(first) * time.Duration(i) / time.Duration(count-1)).Format("15:04:05")
		}
		label := widget.NewLabel(text)
		label.Alignment = fyne.TextAlignCenter
		ticks.Add(container.NewThemeOverride(label, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 9}, shade: monitorMutedColor}))
	}
	return shellFixed(shellPanel(container.New(layout.NewCustomPaddedVBoxLayout(8), monitorSection("CPU 负载趋势", "趋势", monitorAccent("CPU")), shellFixed(chart, 1, 72), shellFixed(ticks, 1, 14)), monitorIdentityFill, 12, 12), 0, 144)
}

type monitorTicksLayout struct{}

func (*monitorTicksLayout) MinSize([]fyne.CanvasObject) fyne.Size { return fyne.NewSize(1, 14) }
func (*monitorTicksLayout) Layout(o []fyne.CanvasObject, size fyne.Size) {
	width := size.Width / float32(len(o))
	for i, obj := range o {
		obj.Move(fyne.NewPos(float32(i)*width, 0))
		obj.Resize(fyne.NewSize(width, 14))
	}
}

type monitorDetailRow struct {
	widget.BaseWidget
	title, left, right string
	fraction           float64
	network            bool
}

func newMonitorDetailRow(title, left, right string, fraction float64, network bool) *monitorDetailRow {
	r := &monitorDetailRow{title: title, left: left, right: right, fraction: fraction, network: network}
	r.ExtendBaseWidget(r)
	return r
}
func (r *monitorDetailRow) CreateRenderer() fyne.WidgetRenderer {
	label := func(text string, size float32, align fyne.TextAlign, shade color.Color) fyne.CanvasObject {
		l := widget.NewLabel(text)
		l.Alignment = align
		l.Truncation = fyne.TextTruncateEllipsis
		return container.NewThemeOverride(l, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: size}, shade: shade})
	}
	shade := color.Color(monitorMutedColor)
	if r.network {
		shade = monitorAccent("网络")
	}
	objects := []fyne.CanvasObject{shellRectangle(monitorIdentityFill, 8, shellBorderColor), label(r.title, 11, fyne.TextAlignLeading, monitorMutedColor), label(r.left, 11, fyne.TextAlignTrailing, shade), label(r.right, 11, fyne.TextAlignTrailing, monitorBlueColor), shellRectangle(monitorTrackColor, 2, nil), shellRectangle(monitorAccent("磁盘"), 2, nil)}
	if r.network {
		objects[4].Hide()
		objects[5].Hide()
	}
	return &monitorDetailRenderer{row: r, objects: objects}
}

type monitorDetailRenderer struct {
	row     *monitorDetailRow
	objects []fyne.CanvasObject
}

func (r *monitorDetailRenderer) MinSize() fyne.Size {
	if r.row.network {
		return fyne.NewSize(280, 38)
	}
	return fyne.NewSize(280, 46)
}
func (r *monitorDetailRenderer) Layout(size fyne.Size) {
	place := func(i int, x, y, w, h float32) {
		r.objects[i].Move(fyne.NewPos(x, y))
		r.objects[i].Resize(fyne.NewSize(max(0, w), h))
	}
	place(0, 0, 0, size.Width, size.Height)
	if r.row.network {
		for _, column := range []struct {
			index    int
			x, width float32
		}{{1, 12, size.Width - 135}, {2, size.Width - 120, 51}, {3, size.Width - 65, 53}} {
			height := r.objects[column.index].MinSize().Height
			place(column.index, column.x, (size.Height-height)/2, column.width, height)
		}
	} else {
		place(1, 12, 7, size.Width-85, 20)
		place(2, size.Width-72, 7, 60, 20)
		place(4, 11, 31, size.Width-22, 4)
		place(5, 11, 31, (size.Width-22)*float32(min(1, max(0, r.row.fraction))), 4)
	}
}
func (r *monitorDetailRenderer) Refresh()                     { r.Layout(r.row.Size()); canvas.Refresh(r.row) }
func (r *monitorDetailRenderer) Objects() []fyne.CanvasObject { return r.objects }
func (*monitorDetailRenderer) Destroy()                       {}

type monitorSectionLayout struct{}

func (*monitorSectionLayout) MinSize(o []fyne.CanvasObject) fyne.Size {
	return fyne.NewSize(18+o[1].MinSize().Width, 20)
}
func (*monitorSectionLayout) Layout(o []fyne.CanvasObject, size fyne.Size) {
	o[0].Move(fyne.NewPos(0, (size.Height-12)/2))
	o[0].Resize(fyne.NewSize(12, 12))
	height := o[1].MinSize().Height
	// Arial has extra CJK fallback leading; do not apply that correction
	// when the desktop uses DejaVu, Segoe or the bundled fallback font.
	offset := float32(0)
	if strings.Contains(desktopFonts.bold.Name(), "/Arial ") {
		offset = -3
	}
	o[1].Move(fyne.NewPos(18, (size.Height-height)/2+offset))
	o[1].Resize(fyne.NewSize(max(0, size.Width-18), height))
}
