package filedialog

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"time"
)

// OpenLocal opens a downloaded file with the operating system's associated app.
func OpenLocal(ctx context.Context, filename string) error {
	absolute, err := filepath.Abs(filename)
	if err != nil {
		return err
	}
	info, err := os.Stat(absolute)
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() {
		return errors.New("只支持打开普通文件")
	}
	name, args := localOpenCommand(runtime.GOOS, absolute)
	if err := ctx.Err(); err != nil {
		return err
	}
	// Some Linux associations keep the launcher alive as long as the editor.
	// Reap it asynchronously so the file pane stays usable after opening a file.
	command := exec.Command(name, args...)
	if err := command.Start(); err != nil {
		return err
	}
	done := make(chan error, 1)
	go func() { done <- command.Wait() }()
	timer := time.NewTimer(750 * time.Millisecond)
	defer timer.Stop()
	select {
	case err := <-done:
		return err
	case <-timer.C:
		return nil
	case <-ctx.Done():
		command.Process.Kill()
		return ctx.Err()
	}
}
func localOpenCommand(platform, filename string) (string, []string) {
	switch platform {
	case "darwin":
		return "open", []string{filename}
	case "windows":
		return "rundll32.exe", []string{"url.dll,FileProtocolHandler", filename}
	default:
		return "xdg-open", []string{filename}
	}
}
