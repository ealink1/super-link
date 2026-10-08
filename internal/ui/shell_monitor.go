package ui

import (
	"context"
	"fmt"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellMonitor struct {
	content *fyne.Container
	refresh *shellAlignedButton
	status  *widget.Label
	values  [4]*widget.Label
	details [4]*widget.Label
	memory  *widget.ProgressBar
	disk    *widget.ProgressBar
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
}

func newShellMonitor(p *shellPane, remote *transport.Remote) *shellMonitor {
	m := newShellMonitorView(p)
	m.refresh.OnTapped = func() {
		if p.closed || p.ended {
			return
		}
		m.refresh.Disable()
		m.status.SetText("正在读取服务器指标…")
		p.workspace.owner.jobs.run(func(context.Context) (any, error) { return remote.Monitor(p.ctx) }, func(value any, err error) {
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
	m.refresh.OnTapped()
	return m
}

func newShellMonitorView(p *shellPane) *shellMonitor {
	m := &shellMonitor{status: widget.NewLabel("等待读取服务器指标"), memory: widget.NewProgressBar(), disk: widget.NewProgressBar()}
	m.memory.TextFormatter = func() string { return "" }
	m.disk.TextFormatter = func() string { return "" }
	m.status.Wrapping = fyne.TextWrapWord
	m.refresh = shellButton("刷新", "refresh-cw", false, nil)
	name, endpoint := "SSH 主机", ""
	if p.host != nil {
		name = p.host.Name
		endpoint = fmt.Sprintf("%s@%s:%d", p.host.User, p.host.Host, p.host.Port)
	}
	identity := widget.NewLabel(name)
	identity.TextStyle.Bold, identity.Truncation = true, fyne.TextTruncateEllipsis
	address := widget.NewLabel(endpoint)
	address.Truncation, address.Importance = fyne.TextTruncateEllipsis, widget.LowImportance
	intro := shellVBox(shellPanel(shellBorder(nil, nil, shellHBox(shellImage("server", true, 24), shellFixed(layout.NewSpacer(), 10, 0)), nil, shellVBox(shellLabel(identity, 15), shellFixed(layout.NewSpacer(), 0, 6), shellLabel(address, 11))), shellBackground, 10, 12), shellFixed(layout.NewSpacer(), 0, 12))
	load := m.card(0, "CPU 负载", nil)
	memory := m.card(1, "内存", m.memory)
	disk := m.card(2, "根目录磁盘", m.disk)
	uptime := m.card(3, "运行时间", nil)
	cards := shellVBox(shellColumns(8, load, memory), shellFixed(layout.NewSpacer(), 0, 8), shellColumns(8, disk, uptime))
	info := shellVBox(shellFixed(layout.NewSpacer(), 0, 18), shellText("指标说明", 12, true, shellTextColor), shellFixed(layout.NewSpacer(), 0, 8), shellLabel(widget.NewLabel("Linux 实时快照\nCPU 负载为 1 / 5 / 15 分钟平均值"), 11))
	body := container.NewVScroll(shellInset(shellVBox(intro, cards, info), 12))
	footer := shellInset(shellVBox(shellButtonView(m.refresh), shellLabel(m.status, 11)), 10)
	m.content = shellBorder(nil, footer, nil, nil, body)
	return m
}

func (m *shellMonitor) card(index int, title string, meter fyne.CanvasObject) fyne.CanvasObject {
	m.values[index], m.details[index] = widget.NewLabel("—"), widget.NewLabel("等待采样")
	m.values[index].TextStyle.Bold = true
	m.details[index].Importance, m.details[index].Wrapping = widget.LowImportance, fyne.TextWrapWord
	content := shellVBox(shellText(title, 11, false, shellMutedColor), shellFixed(layout.NewSpacer(), 0, 8), shellLabel(m.values[index], 18), shellFixed(layout.NewSpacer(), 0, 6), shellLabel(m.details[index], 10))
	if meter != nil {
		content.Add(shellFixed(layout.NewSpacer(), 0, 8))
		content.Add(shellFixed(meter, 0, 5))
	}
	return shellFixed(shellPanel(content, shellBackground, 8, 12), 0, 120)
}

func (m *shellMonitor) apply(metrics transport.Metrics) {
	values, details, memory, disk := shellMetricDisplay(metrics)
	for i := range values {
		m.values[i].SetText(values[i])
		m.details[i].SetText(details[i])
	}
	m.memory.SetValue(memory)
	m.disk.SetValue(disk)
}
