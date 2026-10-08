package ui

import (
	"os"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func TestMonitorTabsAndBoundedHistory(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	p := w.switcher.shell.newPane("fixture")
	defer p.stop()
	m := newShellMonitorView(p)
	now := time.Now()
	for i := 0; i < 100; i++ {
		m.apply(transport.Metrics{System: "Linux", MemoryTotal: 1024, MemoryUsed: 512, Details: transport.MonitorDetails{Sampled: now.Add(time.Duration(i) * time.Second), CPUs: []transport.CPUCounter{{Name: "cpu", Total: uint64(1000 + i*100), Idle: uint64(500 + i*50), User: uint64(250 + i*30)}}, Networks: []transport.NetworkCounter{{Name: "eth0", Received: uint64(i * 1000), Sent: uint64(i * 500)}}}})
	}
	if len(m.history) > 61 || len(m.networkHistory) > 61 || m.cpuValue() != "50.0%" {
		t.Fatal(len(m.history), len(m.networkHistory), m.cpuValue())
	}
	m.metrics.Load = "0.12 0.24 0.36"
	m.metrics.Details.Kernel = "Linux 6.8.0"
	m.metrics.Details.Partitions = []transport.Partition{{Device: "/dev/vda1", Mount: "/", Total: 100 << 30, Used: 50 << 30, Percent: 50}}
	m.metrics.Details.Processes = []transport.ProcessMetric{{PID: 42, Name: "worker", User: "tester", CPU: 12, Memory: 8, RSS: 128 << 20, Start: 999}}
	m.metrics.Details.Listeners = []transport.Listener{{Protocol: "tcp", Address: "0.0.0.0:22"}}
	m.metrics.Details.Addresses = map[string]string{"eth0": "192.0.2.1/24"}
	w.Window.SetContent(p.workspace.content)
	p.showAuxiliary("monitor", "服务器监控", m.content)
	for i := 0; i < 6; i++ {
		m.tabs.SelectIndex(i)
		if os.Getenv("SUPERLINK_UI_CAPTURE_DIR") != "" {
			captureShellFixture(t, w, "monitor-tab-"+m.tabs.Items[i].Text+".png")
		}
		if len(m.body.Objects) != 1 {
			t.Fatal("tab has no content")
		}
	}
	m.setDirectorySummary("/home/tester", []transport.File{{Name: "folder", Directory: true}, {Name: "a", Size: 1024}})
	if m.fileSummary.Text != "PATH · /home/tester\nDIR · 1   FILE · 1   TOTAL · 1.0 KiB" {
		t.Fatal(m.fileSummary.Text)
	}
	test.TapAt(m.auto, fyne.NewPos(m.auto.Size().Width/2, m.auto.Size().Height/2))
	if m.auto.Checked {
		t.Fatal("automatic sampling cannot be paused")
	}
}
