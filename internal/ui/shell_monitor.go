package ui

import (
	"context"
	"fmt"
	"image/color"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellMonitor struct {
	content                   *fyne.Container
	pane                      *shellPane
	remote                    *transport.Remote
	refresh                   *shellAlignedButton
	status                    *widget.Label
	body                      *fyne.Container
	tabs                      *container.AppTabs
	auto                      *widget.Check
	interval                  time.Duration
	lastRefresh               time.Time
	busy                      bool
	metrics                   transport.Metrics
	rates                     transport.MonitorRates
	history                   []monitorPoint
	networkHistory            []monitorPoint
	fileSummary               *widget.Label
	scanPaths                 *widget.Entry
	scanMinimum, scanCount    *widget.Select
	scanButton                *shellAlignedButton
	scanStatus                *widget.Label
	scanResults               *fyne.Container
	scanCancel                context.CancelFunc
	navigation, footer        *fyne.Container
	summaryDirectory          string
	summaryDirs, summaryFiles int
	summaryBytes              int64
}

func (p *shellPane) showMonitor() {
	remote, ok := p.session.(*transport.Remote)
	if !ok || p.ended {
		p.status.SetText("服务器监控需要已连接的 Linux SSH 会话")
		return
	}
	if p.monitorPane == nil {
		p.monitorPane = newShellMonitor(p, remote)
	}
	p.showAuxiliary("monitor", "服务器监控", p.monitorPane.content)
	if p.auxKind == "monitor" {
		p.monitorPane.sample()
	}
}
func newShellMonitor(p *shellPane, remote *transport.Remote) *shellMonitor {
	m := newShellMonitorView(p)
	m.remote = remote
	if p.filePane == nil {
		p.filePane = newShellFiles(p, remote)
	}
	p.workspace.owner.jobs.run(func(context.Context) (any, error) {
		ticker := time.NewTicker(time.Second)
		defer ticker.Stop()
		for {
			select {
			case <-p.ctx.Done():
				return nil, nil
			case <-ticker.C:
				p.workspace.owner.jobs.dispatch(func() {
					if !p.closed && !p.ended && p.auxKind == "monitor" && p.workspace.owner.switcher.mode == 1 && p.workspace.tabs.Selected() == p.item && m.auto.Checked && time.Since(m.lastRefresh) >= m.interval {
						m.sample()
					}
				})
			}
		}
	}, func(any, error) {})
	return m
}
func newShellMonitorView(p *shellPane) *shellMonitor {
	m := &shellMonitor{pane: p, status: widget.NewLabel("等待读取服务器指标"), body: container.NewStack(), interval: 3 * time.Second}
	m.status.Wrapping = fyne.TextWrapWord
	m.fileSummary = widget.NewLabel("PATH · —   DIR · —   FILE · —   TOTAL · —")
	m.fileSummary.Wrapping = fyne.TextWrapWord
	m.refresh = shellButton("刷新", "refresh-cw", false, m.sample)
	m.auto = widget.NewCheck("自动刷新", nil)
	m.auto.SetChecked(true)

	m.tabs = container.NewAppTabs()
	for _, name := range []string{"综合", "CPU", "GPU", "MEM", "DISK", "NET"} {
		m.tabs.Append(container.NewTabItem(name, container.NewStack()))
	}
	m.tabs.OnSelected = func(*container.TabItem) { m.render() }
	m.initScan()
	m.navigation, m.footer = container.NewStack(), container.NewStack()
	m.content = container.NewStack(shellRectangle(monitorSurfaceColor, 0, nil), shellBorder(shellVBox(m.navigation, shellLine()), m.footer, nil, nil, container.NewVScroll(shellInset(m.body, 16))))
	m.render()
	return m
}
func (m *shellMonitor) sample() {
	p := m.pane
	if m.remote == nil || m.busy || p.closed || p.ended {
		return
	}
	m.busy = true
	m.lastRefresh = time.Now()
	m.refresh.Disable()
	p.workspace.owner.jobs.run(func(context.Context) (any, error) { return m.remote.Monitor(p.ctx) }, func(value any, err error) {
		m.busy = false
		m.refresh.Enable()
		if p.closed || p.ended {
			return
		}
		if err != nil {
			m.status.SetText("读取失败：" + err.Error())
			return
		}
		m.apply(value.(transport.Metrics))
		m.status.SetText("更新于 " + time.Now().Format("15:04:05"))
	})
}
func (m *shellMonitor) apply(metrics transport.Metrics) {
	m.rates = transport.Rates(m.metrics.Details, metrics.Details)
	m.metrics = metrics
	if len(m.rates.CPUs) > 0 {
		c := m.rates.CPUs[0]
		m.history = append(m.history, monitorPoint{metrics.Details.Sampled, c.User, c.System, c.Used})
	}
	cutoff := metrics.Details.Sampled.Add(-max(60*time.Second, 60*m.interval))
	for len(m.history) > 0 && m.history[0].Time.Before(cutoff) {
		m.history = m.history[1:]
	}
	if len(m.history) > 61 {
		m.history = m.history[len(m.history)-61:]
	}
	down, up := monitorHostNetworkTotals(m.rates.Networks)
	if m.rates.Ready {
		m.networkHistory = append(m.networkHistory, monitorPoint{Time: metrics.Details.Sampled, User: down, System: up})
	}
	for len(m.networkHistory) > 0 && m.networkHistory[0].Time.Before(cutoff) {
		m.networkHistory = m.networkHistory[1:]
	}
	if len(m.networkHistory) > 61 {
		m.networkHistory = m.networkHistory[len(m.networkHistory)-61:]
	}
	m.render()
}
func (m *shellMonitor) render() {
	var body fyne.CanvasObject
	switch m.tabs.SelectedIndex() {
	case 1:
		body = m.cpuView()
	case 2:
		body = m.gpuView()
	case 3:
		body = m.memoryView()
	case 4:
		body = m.diskView()
	case 5:
		body = m.networkView()
	default:
		body = m.overview()
	}
	m.navigation.Objects = []fyne.CanvasObject{m.navigationView()}
	m.navigation.Refresh()
	m.footer.Objects = []fyne.CanvasObject{m.footerView()}
	m.footer.Refresh()
	m.body.Objects = []fyne.CanvasObject{body}
	m.body.Refresh()
}
func monitorLabel(text string) *widget.Label {
	l := widget.NewLabel(text)
	l.Wrapping = fyne.TextWrapWord
	return l
}
func monitorCard(title, value, detail string, fraction float64, action func()) fyne.CanvasObject {
	bar := widget.NewProgressBar()
	bar.TextFormatter = func() string { return "" }
	bar.SetValue(fraction)
	headingLabel := monitorLabel(title)
	headingLabel.Importance = widget.LowImportance
	heading := shellLabel(headingLabel, 11)
	valueLabel := widget.NewLabel(value)
	valueLabel.TextStyle.Bold = true
	valueLabel.Wrapping = fyne.TextWrapWord
	content := monitorVBox(heading, container.NewThemeOverride(valueLabel, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 18}, shade: monitorAccent(title)}), shellLabel(monitorLabel(detail), 10))
	if fraction >= 0 {
		content.Add(shellFixed(container.NewThemeOverride(bar, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme()}, shade: monitorAccent(title)}), 1, 5))
	}
	if action != nil {
		content.Add(shellButtonView(shellButton("查看详情", "", false, action)))
	}
	return shellPanel(content, shellBackground, 8, 10)
}
func monitorBytes(n uint64) string { return shellFileSize(int64(n)) }
func monitorRatio(used, total uint64) float64 {
	if total == 0 {
		return 0
	}
	return min(1, float64(used)/float64(total))
}
func (m *shellMonitor) selectTab(i int) func() { return func() { m.tabs.SelectIndex(i) } }
func (m *shellMonitor) cpuValue() string {
	if len(m.rates.CPUs) == 0 {
		return "采样中"
	}
	return fmt.Sprintf("%.1f%%", m.rates.CPUs[0].Used)
}

func (m *shellMonitor) setDirectorySummary(directory string, files []transport.File) {
	var dirs, count int
	var total int64
	for _, f := range files {
		if f.Directory {
			dirs++
		} else {
			count++
			if f.Size > 0 {
				total += f.Size
			}
		}
	}
	m.summaryDirectory, m.summaryDirs, m.summaryFiles, m.summaryBytes = directory, dirs, count, total
	if m.footer != nil {
		m.footer.Objects = []fyne.CanvasObject{m.footerView()}
		m.footer.Refresh()
	}
	m.fileSummary.SetText(fmt.Sprintf("PATH · %s\nDIR · %d   FILE · %d   TOTAL · %s", directory, dirs, count, shellFileSize(total)))
}

func monitorVBox(objects ...fyne.CanvasObject) *fyne.Container {
	return container.New(layout.NewCustomPaddedVBoxLayout(6), objects...)
}

type monitorTintTheme struct {
	shellLabelTheme
	shade color.Color
}

func (t monitorTintTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground || name == theme.ColorNamePrimary {
		return t.shade
	}
	return t.shellLabelTheme.Color(name, variant)
}
func monitorAccent(title string) color.Color {
	switch {
	case strings.Contains(title, "内存") || strings.Contains(title, "Swap"):
		return color.NRGBA{R: 231, G: 107, B: 204, A: 255}
	case strings.Contains(title, "磁盘") || strings.HasPrefix(title, "/"):
		return color.NRGBA{R: 227, G: 145, B: 20, A: 255}
	case strings.Contains(title, "网络"):
		return color.NRGBA{R: 48, G: 163, B: 28, A: 255}
	default:
		return color.NRGBA{R: 17, G: 151, B: 161, A: 255}
	}
}

// Keep monitoring's code-style labels close to the reference on macOS instead
// of inheriting the very thin Courier font used by legacy terminal views.
var monitorMonoFont = loadDesktopFont([]string{"/System/Library/Fonts/SFNSMono.ttf"}, desktopFonts.mono)

func (t monitorTintTheme) Font(style fyne.TextStyle) fyne.Resource {
	if style.Monospace {
		return monitorMonoFont
	}
	return t.shellLabelTheme.Font(style)
}
