package ui

import (
	"context"
	"errors"
	"strings"
	"sync"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func TestShellConnectModelBoundsNotificationsAndCloses(t *testing.T) {
	m := newShellConnectModel()
	var workers sync.WaitGroup
	for range 4 {
		workers.Add(1)
		go func() {
			defer workers.Done()
			for i := range 1000 {
				m.record(transport.ConnectEvent{Stage: transport.ConnectStage(i % 4), State: transport.ConnectStarted})
				_ = m.snapshot()
			}
		}()
	}
	workers.Wait()
	if len(m.snapshot()) > shellConnectLogLimit || len(m.changes) != 1 {
		t.Fatal("unbounded progress queue")
	}
	m.close()
	before := len(m.snapshot())
	m.record(transport.ConnectEvent{Stage: transport.ConnectReady, State: transport.ConnectCompleted})
	if len(m.snapshot()) != before {
		t.Fatal("closed observer accepted an event")
	}
	m.close()
}

func TestShellConnectDialogCancellationAndFailureStates(t *testing.T) {
	for _, cancel := range []bool{false, true} {
		t.Run(map[bool]string{false: "authentication failure", true: "user cancellation"}[cancel], func(t *testing.T) {
			w := shellTestWindow(t)
			w.switcher.selectMode(1)
			waitUI(t, w)
			p := w.switcher.shell.newPane("SSH fixture")
			d := newShellConnectDialog(p)
			p.connection = d
			d.model.record(transport.ConnectEvent{Stage: transport.ConnectTCP, State: transport.ConnectCompleted})
			d.model.record(transport.ConnectEvent{Stage: transport.ConnectAuth, State: transport.ConnectStarted})
			pumpShell(t, w, func() bool { return d.steps[transport.ConnectAuth].state == int(transport.ConnectStarted) })
			if cancel {
				test.Tap(d.action)
				if !errors.Is(p.ctx.Err(), context.Canceled) || !d.hidden || !p.ended {
					t.Fatal("cancel left connection alive")
				}
			} else {
				d.model.record(transport.ConnectEvent{Stage: transport.ConnectAuth, State: transport.ConnectFailed})
				p.finish(errors.New("authentication rejected"))
				if d.hidden || !d.retry.Visible() || d.steps[transport.ConnectAuth].state != int(transport.ConnectFailed) || d.action.Text != "关闭" {
					t.Fatal("failure lost its stage or retry action")
				}
				test.Tap(d.action)
			}
			if p.indicator.status != application.ConnectionFailed || !p.terminal.closed {
				t.Fatal("ended connection stayed live")
			}
			p.stop()
			pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
		})
	}
}

func TestShellConnectionCompletionAndSidePanelGeometry(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	w.Window.Resize(fyne.NewSize(960, 640))
	p := w.switcher.shell.newPane("fixture")
	d := newShellConnectDialog(p)
	p.connection = d
	for stage := transport.ConnectTCP; stage <= transport.ConnectReady; stage++ {
		d.model.record(transport.ConnectEvent{Stage: stage, State: transport.ConnectCompleted})
	}
	d.finish(nil)
	if !d.hidden || d.steps[transport.ConnectReady].state != int(transport.ConnectCompleted) {
		t.Fatal("ready did not dismiss the connection dialog")
	}
	m := newShellMonitorView(p)
	p.showAuxiliary("monitor", "服务器监控", m.content)
	if p.aux.Size().Height < 400 || p.aux.Size().Width != 320 {
		t.Fatalf("side panel lost full height: %v", p.aux.Size())
	}
	p.showAuxiliary("monitor", "服务器监控", m.content)
	if p.aux.Visible() {
		t.Fatal("second monitor click did not collapse panel")
	}
	if !p.auxHandle.Visible() || p.auxKind != "" {
		t.Fatal("collapsed panel lost its expand handle or stayed active")
	}
	for _, size := range []fyne.Size{fyne.NewSize(960, 640), fyne.NewSize(1320, 890)} {
		w.Window.Resize(size)
		body := p.item.Content.(*fyne.Container).Objects[0].(*fyne.Container)
		handle := p.auxHandle
		if handle.Position().X+handle.Size().Width != body.Size().Width || handle.Position().Y+handle.Size().Height/2 != body.Size().Height/2 {
			t.Fatalf("expand handle is not centered at the right edge: %v %v, body %v", handle.Position(), handle.Size(), body.Size())
		}
	}
	panel := p.aux.Objects[0]
	test.Tap(p.auxExpand)
	if !p.aux.Visible() || p.auxHandle.Visible() || p.auxKind != "monitor" || p.aux.Objects[0] != panel {
		t.Fatal("expand did not restore the existing monitor panel")
	}
	p.showAuxiliary("files", "文件管理 · SFTP", widget.NewLabel("fixture files"))
	p.hideAuxiliary()
	test.Tap(p.auxExpand)
	if p.auxKind != "files" || !p.aux.Visible() || p.auxHandle.Visible() {
		t.Fatal("expand restored a previous panel instead of the last collapsed panel")
	}
	p.terminal.feed([]byte("keep / 你好"))
	before, font := p.terminal.emulator, p.terminal.textSize
	p.adjustFont(-1)
	if p.terminal.textSize != font-1 || p.terminal.emulator != before || !strings.Contains(p.terminal.emulator.String(), "keep / 你好") {
		t.Fatal("font change replaced terminal")
	}
	p.stop()
	pumpShell(t, w, func() bool { return w.jobs.workers.Load() == 0 })
}
