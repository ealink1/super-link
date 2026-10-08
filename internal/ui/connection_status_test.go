package ui

import (
	"context"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
)

func waitConnectionStatus(t *testing.T, w *Window, id string, want application.ConnectionStatus) {
	t.Helper()
	deadline := time.NewTimer(time.Second)
	defer deadline.Stop()
	queueValue, ok := testUIQueues.Load(w)
	if !ok {
		t.Fatal("window has no UI queue")
	}
	queue := queueValue.(chan func())
	for w.sidebar.connectionStatuses[id] != want {
		select {
		case update := <-queue:
			update()
		case <-deadline.C:
			t.Fatalf("UI status = %v, want %v", w.sidebar.connectionStatuses[id], want)
		}
	}
}

func TestConnectionIndicatorFollowsQueriesAndDisconnectWithoutExpandingTree(t *testing.T) {
	w, p := parityWindow(t)
	n := w.sidebar
	row := newTreeRow(n)
	root := n.nodes[n.roots[0]]
	row.bind(root)
	if row.status.Visible() || row.status.status != application.ConnectionDisconnected {
		t.Fatal("unconnected saved connection displays a status dot")
	}
	w.jobs.run(func(ctx context.Context) (any, error) {
		return w.Engine.Execute(ctx, p.ID, domain.Execution{Text: "SELECT 1"})
	}, func(_ any, err error) {
		if err != nil {
			t.Error(err)
		}
	})
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionConnected)
	row.bind(root)
	if row.status.status != application.ConnectionConnected || n.tree.IsBranchOpen(root.id) {
		t.Fatal("query connection did not update a collapsed tree")
	}
	w.selected = p.ID
	w.disconnectSelected()
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionDisconnected)
	row.bind(root)
	if row.status.status != application.ConnectionDisconnected {
		t.Fatal("disconnect left the reused row green")
	}
}

func TestConnectionIndicatorRowReuseAndThemeLayout(t *testing.T) {
	w, p := parityWindow(t)
	n := w.sidebar
	row := newTreeRow(n)
	root := n.nodes[n.roots[0]]
	n.connectionStatuses[p.ID] = application.ConnectionConnected
	row.bind(root)
	row.Resize(fyne.NewSize(220, 28))
	for _, dark := range []bool{false, true} {
		w.App.Settings().SetTheme(Theme{Dark: dark})
		row.Refresh()
		row.status.Refresh()
		statusColor := color.NRGBAModel.Convert(row.status.center.FillColor).(color.NRGBA)
		if statusColor.G <= statusColor.R || statusColor.G <= statusColor.B || statusColor.A != 255 {
			t.Fatal("connected dot is not visibly green", dark, statusColor)
		}
		if row.status.Position().X+row.status.Size().Width > row.Size().Width || row.label.Size().Width <= 0 {
			t.Fatal("status overlaps the row edge or hides its name")
		}
		window := w.App.NewWindow("Connection indicator preview")
		offline := newTreeRow(n)
		offline.bind(root)
		offline.label.SetText("未连接")
		offline.status.setStatus(application.ConnectionDisconnected)
		pending := newTreeRow(n)
		pending.bind(root)
		pending.label.SetText("正在连接")
		pending.status.setStatus(application.ConnectionConnecting)
		window.SetContent(container.NewVBox(row, offline, pending))
		window.Resize(fyne.NewSize(250, 100))
		name := "connection-status-light.png"
		if dark {
			name = "connection-status-dark.png"
		}
		captureConnectionIndicators(t, window, name)
		window.Close()
	}
	row.bind(&navNode{kind: "database", profileID: p.ID, label: "main"})
	if row.status.Visible() {
		t.Fatal("database row displays a connection status dot")
	}
	for _, kind := range []string{"object", "category", "schema", "message"} {
		row.bind(&navNode{kind: kind, profileID: p.ID, label: "item"})
		if row.status.Visible() {
			t.Fatal("reused row retained a dot", kind)
		}
	}
	row.bind(nil)
	if row.status.Visible() || row.label.Text != "" {
		t.Fatal("empty row retained connection data")
	}
}

func captureConnectionIndicators(t *testing.T, window fyne.Window, name string) {
	t.Helper()
	directory := os.Getenv("SUPERLINK_UI_CAPTURE_DIR")
	if directory == "" {
		return
	}
	if err := os.MkdirAll(directory, 0700); err != nil {
		t.Fatal(err)
	}
	file, err := os.OpenFile(filepath.Join(directory, name), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0600)
	if err != nil {
		t.Fatal(err)
	}
	err = png.Encode(file, window.Canvas().Capture())
	closeErr := file.Close()
	if err != nil || closeErr != nil {
		t.Fatal("capture failed", err, closeErr)
	}
}

func TestConnectionIndicatorShowsPendingThenFailure(t *testing.T) {
	w, p := parityWindow(t)
	started, proceed := make(chan struct{}), make(chan struct{})
	var once sync.Once
	t.Cleanup(func() { once.Do(func() { close(proceed) }) })
	w.Engine.Factory = func(ctx context.Context, _ domain.Profile) (adapter.Client, error) {
		close(started)
		select {
		case <-proceed:
			return nil, domain.ErrClosed
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	w.jobs.run(func(ctx context.Context) (any, error) {
		return w.Engine.Scopes(ctx, p.ID)
	}, func(_ any, err error) {
		if err == nil {
			t.Error("failed connection accepted")
		}
	})
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("driver was not invoked")
	}
	waitConnectionStatus(t, w, p.ID, application.ConnectionConnecting)
	once.Do(func() { close(proceed) })
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionFailed)
}

func TestConnectionIndicatorDisconnectClearsCachedMetadataForReopen(t *testing.T) {
	w, p := parityWindow(t)
	n := w.sidebar
	root := n.nodes[n.roots[0]]
	n.tree.OpenBranch(root.id)
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionConnected)
	if !root.loaded || len(root.children) == 0 {
		t.Fatal("database list was not cached")
	}
	w.selected = p.ID
	w.disconnectSelected()
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionDisconnected)
	if root.loaded || len(root.children) != 0 || n.tree.IsBranchOpen(root.id) {
		t.Fatal("disconnected branch retained metadata that bypasses reconnection")
	}
	n.tree.OpenBranch(root.id)
	waitUI(t, w)
	waitConnectionStatus(t, w, p.ID, application.ConnectionConnected)
	if !root.loaded || len(root.children) == 0 {
		t.Fatal("reopen did not reconnect and reload")
	}
}
