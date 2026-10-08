package ui

import (
	"math"
	"os"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"github.com/ealink1/super-link/internal/domain"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

// Optional native software capture at the reference's 420x829 logical viewport.
func TestMonitorReferenceCapture(t *testing.T) {
	if os.Getenv("SUPERLINK_UI_CAPTURE_DIR") == "" {
		t.Skip("optional visual fixture")
	}
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	p := w.switcher.shell.newPane("SSH · ysy")
	defer p.stop()
	p.host = &domain.ShellHost{Name: "ysy", Host: "192.168.0.104", User: "kaifa", Port: 22}
	m := newShellMonitorView(p)
	m.metrics = transport.Metrics{System: "Linux", Uptime: "26400 0", MemoryUsed: 10093173146, MemoryTotal: 16750372454, Details: transport.MonitorDetails{Kernel: "Linux kaifaserver 6.8.0-142-generic", Partitions: []transport.Partition{{Device: "/dev/vda1", Mount: "/", Used: 97 << 30, Total: 194 << 30, Percent: 53}, {Device: "/dev/vda2", Mount: "/boot", Used: 200 << 20, Total: 2 << 30, Percent: 11}}, Networks: []transport.NetworkCounter{{Name: "enp1s0"}}}}
	m.rates = transport.MonitorRates{Ready: true, CPUs: []transport.CPURate{{Name: "cpu", Used: 25.2, User: 17.1, System: 8}}, Networks: []transport.NetworkRate{{Name: "enp1s0", Received: 4 << 10, Sent: 7 << 10}}}
	end := time.Date(2026, 10, 8, 17, 7, 7, 0, time.Local)
	for i := 0; i < 60; i++ {
		m.history = append(m.history, monitorPoint{Time: end.Add(time.Duration(i-59) * 3 * time.Second), Used: 20 + 4*math.Sin(float64(i)*1.7) + 2*math.Cos(float64(i)*2.3)})
	}
	m.setDirectorySummary("/home/kaifa", []transport.File{{Name: "folder", Directory: true}, {Name: "one", Size: 37 << 20}, {Name: "two", Size: 38629540}})
	m.render()
	w.Window.SetContent(container.NewThemeOverride(m.content, newShellTheme()))
	captureShellFixtureSize(t, w, "monitor-reference-light.png", fyne.NewSize(420, 829))
	w.setDark(true)
	m.render()
	captureShellFixtureSize(t, w, "monitor-reference-dark.png", fyne.NewSize(420, 829))
}
