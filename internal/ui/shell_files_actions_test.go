package ui

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func fileActionFixture(t *testing.T) (*Window, *shellFiles) {
	t.Helper()
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	p := w.switcher.shell.newPane("fixture")
	t.Cleanup(p.stop)
	f := &shellFiles{pane: p, selected: -1, directory: widget.NewEntry(), status: widget.NewLabel("")}
	p.filePane = f
	f.directory.SetText("/home/tester")
	f.content = newShellFilesView(f)
	f.files = []transport.File{{Name: "ott_member_code_pool.zip", Size: 300 << 20, Modified: time.Date(2026, 10, 8, 20, 20, 0, 0, time.Local)}, {Name: "test.txt", Size: 1024}}
	f.filterFiles()
	return w, f
}
func TestShellFileActionTargetsAndPermissions(t *testing.T) {
	_, f := fileActionFixture(t)
	f.list.Select(1)
	file, source, ok := f.selectedFile()
	if !ok || file.Name != "test.txt" || source != "/home/tester/test.txt" {
		t.Fatal(file, source, ok)
	}
	f.search.SetText("missing")
	if _, _, ok := f.selectedFile(); ok {
		t.Fatal("stale target retained")
	}
	for _, name := range []string{"..", ".", "a/b", "a\\b", "bad\x00name"} {
		if validShellFileName(name) {
			t.Fatal(name)
		}
	}
	for _, raw := range []string{"999", "7777", "64", "-01", "64a"} {
		if _, err := parseShellPermissions(raw); err == nil {
			t.Fatal(raw)
		}
	}
	mode, err := parseShellPermissions("640")
	if err != nil || mode != 0640 || shellFilePermissions("-rw-r-----") != "640" {
		t.Fatal(mode, err)
	}
}
func TestShellFileTransferCompletionAndCancellation(t *testing.T) {
	w, f := fileActionFixture(t)
	f.transfer("下载", "test.txt", 1024, func(context.Context, func(int64)) error { return nil }, nil)
	pumpShell(t, w, func() bool { return !f.busy })
	if f.progress.Value != 1 || !strings.Contains(f.transferLabel.Text, "下载完成") {
		t.Fatal(f.progress.Value, f.transferLabel.Text)
	}
	f.transfer("下载", "test.txt", 1024, func(ctx context.Context, progress func(int64)) error { progress(512); <-ctx.Done(); return ctx.Err() }, nil)
	f.cancel()
	pumpShell(t, w, func() bool { return !f.busy })
	if !strings.Contains(f.transferLabel.Text, "已取消") || !strings.Contains(f.status.Text, "取消") {
		t.Fatal(f.transferLabel.Text, f.status.Text)
	}
}
func TestShellFileMenuReferenceCapture(t *testing.T) {
	if os.Getenv("SUPERLINK_UI_CAPTURE_DIR") == "" {
		t.Skip("optional visual capture")
	}
	w, f := fileActionFixture(t)
	f.list.Select(0)
	f.setTransferProgress("下载", f.files[0].Name, 183<<20, 300<<20)
	f.transferBar.Show()
	w.Window.SetContent(container.NewThemeOverride(f.content, newShellTheme()))
	w.Window.Resize(fyne.NewSize(420, 500))
	f.showMenu(fyne.NewPos(230, 130))
	captureShellFixtureSize(t, w, "files-menu-reference.png", fyne.NewSize(420, 500))
}
