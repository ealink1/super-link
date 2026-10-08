package filedialog

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/ncruces/zenity"
)

func TestNativePickerSelectionAndCancellation(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "file with spaces.txt")
	if err := os.WriteFile(file, []byte("fixture"), 0600); err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct {
		path      string
		directory bool
		wantError bool
	}{{file, false, false}, {dir, true, false}, {dir, false, true}, {file, true, true}} {
		got, err := selectPath(context.Background(), "选择", c.directory, func(...zenity.Option) (string, error) { return c.path, nil })
		if (err != nil) != c.wantError || (!c.wantError && got != c.path) {
			t.Fatal(got, err)
		}
	}
	got, err := selectPath(context.Background(), "", false, func(...zenity.Option) (string, error) { return "", zenity.ErrCanceled })
	if got != "" || err != nil {
		t.Fatal(got, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = selectPath(ctx, "", false, func(...zenity.Option) (string, error) { t.Fatal("canceled picker opened"); return "", nil })
	if !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}
