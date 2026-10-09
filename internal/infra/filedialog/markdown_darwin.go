//go:build darwin

package filedialog

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/ncruces/zenity"
)

func selectNativePath(ctx context.Context, title string, directory bool, extensions []string) (string, error) {
	executable, err := os.Executable()
	if err != nil {
		return "", err
	}
	helper := filepath.Join(filepath.Dir(executable), "SuperLinkFilePicker.app", "Contents", "MacOS", "file-picker")
	mode := "file"
	if directory {
		mode = "directory"
	}
	args := append([]string{mode, title}, extensions...)
	command := exec.CommandContext(ctx, helper, args...)
	out, err := command.Output()
	if ctx.Err() != nil {
		return "", ctx.Err()
	}
	if err != nil {
		return "", err
	}
	return string(out), nil
}

func platformPicker(ctx context.Context, title string, directory bool, extensions []string) func(...zenity.Option) (string, error) {
	return func(...zenity.Option) (string, error) { return selectNativePath(ctx, title, directory, extensions) }
}
