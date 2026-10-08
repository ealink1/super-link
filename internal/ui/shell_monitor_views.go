package ui

import (
	"fmt"
	"sort"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func (m *shellMonitor) overview() fyne.CanvasObject {
	return container.New(layout.NewCustomPaddedVBoxLayout(12), m.identityCard(), m.overviewCards(), m.overviewChart(), m.overviewPartitions(), m.overviewNetworks())
}
func (m *shellMonitor) cpuView() fyne.CanvasObject {
	d := m.metrics.Details
	load := strings.Fields(m.metrics.Load)
	loadText := "—"
	if len(load) >= 3 {
		loadText = strings.Join(load[:3], " / ")
	}
	temp := "N/A"
	if d.Temperature > 0 {
		temp = fmt.Sprintf("%.1f°C", d.Temperature)
	}
	content := monitorVBox(shellColumns(8, monitorCard("总负载", m.cpuValue(), fmt.Sprintf("%d 个核心", max(0, len(d.CPUs)-1)), -1, nil), monitorCard("负载均值 1 / 5 / 15 分钟", loadText, "CPU 温度 · "+temp, -1, nil)))
	if len(m.rates.CPUs) > 0 {
		c := m.rates.CPUs[0]
		content.Add(monitorLabel(fmt.Sprintf("用户态 %.1f%%   内核态 %.1f%%   IO 等待 %.1f%%", c.User, c.System, c.Wait)))
	}
	content.Add(m.chart("负载历史 · 最近 60 秒"))
	content.Add(shellText("核心详情", 12, true, shellTextColor))
	var cores []fyne.CanvasObject
	for _, c := range m.rates.CPUs {
		if c.Name == "cpu" {
			continue
		}
		cores = append(cores, monitorCard(strings.Replace(c.Name, "cpu", "#", 1), fmt.Sprintf("%.0f%%", c.Used), "", c.Used/100, nil))
		if len(cores) == 4 {
			content.Add(shellColumns(6, cores...))
			cores = nil
		}
	}
	if len(cores) > 0 {
		content.Add(shellColumns(6, cores...))
	}
	content.Add(m.processes(false))
	return content
}
func (m *shellMonitor) gpuView() fyne.CanvasObject {
	content := monitorVBox()
	if len(m.metrics.Details.GPUs) == 0 {
		content = monitorVBox(shellText("未检测到 NVIDIA GPU", 14, true, shellTextColor), monitorLabel("服务器没有可用的 NVIDIA GPU，或未安装 nvidia-smi。安装驱动并重启后，监控会自动识别。"))
		for _, c := range []struct{ name, command string }{{"Ubuntu / Debian", "sudo apt update && sudo apt install -y nvidia-driver-535"}, {"CentOS / RHEL / Fedora", "sudo dnf install -y nvidia-driver"}, {"Arch Linux", "sudo pacman -S nvidia"}} {
			command := c.command
			content.Add(shellPanel(monitorVBox(shellText(c.name, 11, true, shellTextColor), shellLabel(monitorLabel(command), 11), shellButtonView(shellButton("复制安装命令", "copy", false, func() { m.pane.workspace.owner.Window.Clipboard().SetContent(command) }))), shellBackground, 6, 8))
		}
		content.Add(monitorLabel("安装完成后重启服务器，运行 nvidia-smi 验证安装。"))
		return content
	}
	for _, g := range m.metrics.Details.GPUs {
		content.Add(monitorCard(g.Name, fmt.Sprintf("%.0f%%", g.Utilization), fmt.Sprintf("显存 %s / %s · %.0f°C", monitorBytes(g.Used), monitorBytes(g.Total), g.Temperature), g.Utilization/100, nil))
	}
	return content
}
func (m *shellMonitor) memoryView() fyne.CanvasObject {
	d, v := m.metrics.Details, m.metrics
	content := monitorVBox(monitorCard("内存", fmt.Sprintf("%.0f%%", monitorRatio(v.MemoryUsed, v.MemoryTotal)*100), fmt.Sprintf("%s / %s", monitorBytes(v.MemoryUsed), monitorBytes(v.MemoryTotal)), monitorRatio(v.MemoryUsed, v.MemoryTotal), nil))
	fields := []struct {
		name  string
		value uint64
	}{{"可用内存", d.MemoryAvailable}, {"已用内存", v.MemoryUsed}, {"页面缓存 Cached", d.Cached}, {"缓冲区 Buffers", d.Buffers}, {"空闲内存 Free", d.MemoryFree}}
	for _, f := range fields {
		content.Add(monitorLabel(f.name + " · " + monitorBytes(f.value)))
	}
	content.Add(monitorCard("Swap 交换分区", monitorBytes(d.SwapUsed)+" / "+monitorBytes(d.SwapTotal), "", monitorRatio(d.SwapUsed, d.SwapTotal), nil))
	content.Add(m.processes(true))
	return content
}
func (m *shellMonitor) partitions() fyne.CanvasObject {
	content := monitorVBox(shellText("磁盘分区", 12, true, shellTextColor))
	for _, d := range m.metrics.Details.Partitions {
		content.Add(monitorCard(d.Mount, fmt.Sprintf("%.0f%%", d.Percent), d.Device+" · "+monitorBytes(d.Used)+" / "+monitorBytes(d.Total), d.Percent/100, nil))
	}
	return content
}
func (m *shellMonitor) diskView() fyne.CanvasObject {
	content := monitorVBox(m.partitions(), shellText("磁盘 IO · 每秒读写操作", 12, true, shellTextColor))
	for _, d := range m.rates.Disks {
		content.Add(monitorLabel(fmt.Sprintf("%s   读 %.0f IOPS / 写 %.0f IOPS", d.Name, d.Reads, d.Writes)))
	}
	content.Add(m.scanView())
	return content
}
func (m *shellMonitor) networkTotal() string {
	if !m.rates.Ready {
		return "采样中"
	}
	down, up := monitorHostNetworkTotals(m.rates.Networks)
	return fmt.Sprintf("↓ %s/s\n↑ %s/s", monitorBytes(uint64(down)), monitorBytes(uint64(up)))
}
func (m *shellMonitor) networkInterfaces() fyne.CanvasObject {
	content := monitorVBox(shellText("网络接口", 12, true, shellTextColor))
	rates := map[string]transport.NetworkRate{}
	for _, n := range m.rates.Networks {
		rates[n.Name] = n
	}
	for _, n := range m.metrics.Details.Networks {
		r := rates[n.Name]
		content.Add(monitorCard(n.Name, fmt.Sprintf("↓ %s/s · ↑ %s/s", monitorBytes(uint64(r.Received)), monitorBytes(uint64(r.Sent))), fmt.Sprintf(m.metrics.Details.Addresses[n.Name]+" · 累计接收 %s · 累计发送 %s", monitorBytes(n.Received), monitorBytes(n.Sent)), -1, nil))
	}
	return content
}
func (m *shellMonitor) networkView() fyne.CanvasObject {
	d := m.metrics.Details
	content := monitorVBox(shellColumns(6, monitorCard("总连接数", fmt.Sprint(d.Connections), fmt.Sprintf("UDP %d", d.UDPConnections), -1, nil), monitorCard("TCP 连接", fmt.Sprint(d.TCPConnections), fmt.Sprintf("TIME_WAIT %d", d.TimeWait), -1, nil)), monitorCard("文件句柄", fmt.Sprint(d.Handles), "上限 · "+fmt.Sprint(d.HandleLimit), monitorRatio(d.Handles, d.HandleLimit), nil), monitorCard("总吞吐量", m.networkTotal(), "主机接口流量，不含回环及容器内部接口", -1, nil))
	var scale float64 = 1
	for _, p := range m.networkHistory {
		scale = max(scale, p.User, p.System)
	}
	content.Add(shellPanel(monitorVBox(shellText("流量监控 · 最近 60 秒", 12, true, shellTextColor), newMonitorScaledChart(m.networkHistory, scale), monitorTimeline(m.networkHistory), shellText("绿色 下载 / 蓝色 上传 · 峰值 "+monitorBytes(uint64(scale))+"/s", 10, false, shellMutedColor)), shellBackground, 8, 10))
	content.Add(shellText(fmt.Sprintf("监听端口 · %d 项（最多展示 100 项）", len(d.Listeners)), 12, true, shellTextColor))
	for _, l := range d.Listeners {
		listener := l
		content.Add(shellButtonView(shellButton(l.Protocol+" · "+l.Address, "copy", false, func() { m.pane.workspace.owner.Window.Clipboard().SetContent(listener.Address) })))
	}
	content.Add(m.networkInterfaces())
	return content
}
func (m *shellMonitor) processes(memory bool) fyne.CanvasObject {
	title := "进程 CPU 占用排行"
	if memory {
		title = "进程内存占用排行"
	}
	content := monitorVBox(shellText(title, 12, true, shellTextColor))
	processes := append([]transport.ProcessMetric(nil), m.metrics.Details.Processes...)
	sort.SliceStable(processes, func(i, j int) bool {
		if memory {
			return processes[i].RSS > processes[j].RSS
		}
		return processes[i].CPU > processes[j].CPU
	})
	for _, p := range processes[:min(10, len(processes))] {
		proc := p
		label := monitorLabel(fmt.Sprintf("%d · %s · %s\nCPU %.1f%% · MEM %s (%.1f%%)", p.PID, p.Name, p.User, p.CPU, monitorBytes(p.RSS), p.Memory))
		content.Add(shellPanel(monitorVBox(shellLabel(label, 11), shellButtonView(shellButton("停止进程", "", false, func() { m.confirmStop(proc) }))), shellBackground, 6, 8))
	}
	return content
}
func (m *shellMonitor) chart(title string) fyne.CanvasObject {
	return shellPanel(monitorVBox(shellText(title, 11, false, shellMutedColor), newMonitorChart(m.history), monitorTimeline(m.history), shellText("User / Sys · 采样间隔由下方刷新设置控制", 10, false, shellMutedColor)), shellBackground, 8, 10)
}

func monitorTimeline(points []monitorPoint) fyne.CanvasObject {
	if len(points) == 0 {
		return shellText("等待第二次采样", 10, false, shellMutedColor)
	}
	end := points[len(points)-1].Time
	return shellBorder(nil, nil, shellText(points[0].Time.Format("15:04:05"), 10, false, shellMutedColor), shellText(end.Format("15:04:05"), 10, false, shellMutedColor), layout.NewSpacer())
}
