package ui

import (
	"context"
	"fmt"
	"path"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func (m *shellMonitor) initScan() {
	m.scanPaths = widget.NewEntry()
	m.scanPaths.SetText("/home /var /opt")
	m.scanMinimum = widget.NewSelect([]string{"≥ 100 MiB", "≥ 500 MiB", "≥ 1 GiB"}, nil)
	m.scanMinimum.SetSelectedIndex(0)
	m.scanCount = widget.NewSelect([]string{"Top 20", "Top 50", "Top 100"}, nil)
	m.scanCount.SetSelectedIndex(0)
	m.scanStatus = monitorLabel("点击扫描开始分析；最长运行 30 秒")
	m.scanResults = container.NewVBox()
	m.scanButton = shellButton("扫描大文件", "search", false, m.scan)
}
func (m *shellMonitor) scanView() fyne.CanvasObject {
	cancel := shellButton("取消扫描", "x", false, func() {
		if m.scanCancel != nil {
			m.scanCancel()
		}
	})
	return monitorVBox(shellText("大文件分析", 12, true, shellTextColor), shellText("搜索路径 · 用空格分隔多个绝对目录", 10, false, shellMutedColor), m.scanPaths, shellColumns(8, m.scanMinimum, m.scanCount), shellHBox(shellButtonView(m.scanButton), shellButtonView(cancel)), shellLabel(m.scanStatus, 11), m.scanResults)
}
func (m *shellMonitor) scan() {
	if m.remote == nil || m.scanCancel != nil || m.pane.closed || m.pane.ended {
		return
	}
	minimum := []uint64{100 << 20, 500 << 20, 1 << 30}[m.scanMinimum.SelectedIndex()]
	count := []int{20, 50, 100}[m.scanCount.SelectedIndex()]
	paths := strings.Fields(m.scanPaths.Text)
	ctx, cancel := context.WithCancel(m.pane.ctx)
	m.scanCancel = cancel
	m.scanButton.Disable()
	m.scanStatus.SetText("正在扫描…")
	m.pane.workspace.owner.jobs.run(func(context.Context) (any, error) {
		defer cancel()
		return m.remote.LargeFiles(ctx, paths, minimum, count)
	}, func(value any, err error) {
		m.scanCancel = nil
		m.scanButton.Enable()
		if m.pane.closed || m.pane.ended {
			return
		}
		if err != nil {
			m.scanStatus.SetText("扫描失败：" + err.Error())
			return
		}
		files := value.([]transport.LargeFile)
		m.scanStatus.SetText(fmt.Sprintf("找到 %d 个大文件 · 无权限目录会跳过", len(files)))
		m.scanResults.Objects = nil
		for _, f := range files {
			file := f
			m.scanResults.Add(shellPanel(monitorVBox(shellLabel(monitorLabel(file.Path+" · "+monitorBytes(file.Size)), 11), shellHBox(shellButtonView(shellButton("复制路径", "copy", false, func() { m.pane.workspace.owner.Window.Clipboard().SetContent(file.Path) })), shellButtonView(shellButton("所在目录", "folder", false, func() { m.openDirectory(path.Dir(file.Path)) })))), shellBackground, 6, 8))
		}
		m.scanResults.Refresh()
	})
}
func (m *shellMonitor) openDirectory(directory string) {
	p := m.pane
	if p.filePane == nil {
		p.filePane = newShellFiles(p, m.remote)
	}
	p.filePane.directory.SetText(directory)
	p.filePane.refresh()
	p.showAuxiliary("files", "文件管理 · SFTP", p.filePane.content)
}
func (m *shellMonitor) confirmStop(proc transport.ProcessMetric) {
	dialog.ShowConfirm("停止进程", fmt.Sprintf("向 %s（PID %d，用户 %s）发送 SIGTERM？\n此操作可能中断正在运行的服务。", proc.Name, proc.PID, proc.User), func(ok bool) {
		if !ok || m.pane.closed || m.pane.ended {
			return
		}
		m.pane.workspace.owner.jobs.run(func(context.Context) (any, error) { return nil, m.remote.StopProcess(m.pane.ctx, proc) }, func(_ any, err error) {
			if m.pane.closed {
				return
			}
			if err != nil {
				m.status.SetText("停止失败：" + err.Error())
			} else {
				m.status.SetText("已发送 SIGTERM")
				m.sample()
			}
		})
	}, m.pane.workspace.owner.Window)
}
