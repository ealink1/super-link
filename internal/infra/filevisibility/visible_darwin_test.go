//go:build darwin

package filevisibility

import (
	"golang.org/x/sys/unix"
	"os"
	"path/filepath"
	"testing"
)

func TestPrepareClearsHiddenFlagBeforeHardLinkPublication(t *testing.T) {
	path := filepath.Join(t.TempDir(), ".temporary.sql")
	if err := os.WriteFile(path, []byte("SELECT 1;"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := unix.Chflags(path, unix.UF_HIDDEN); err != nil {
		t.Fatal(err)
	}
	if err := Prepare(path); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(filepath.Dir(path), "visible.sql")
	if err := os.Link(path, target); err != nil {
		t.Fatal(err)
	}
	var info unix.Stat_t
	if err := unix.Stat(target, &info); err != nil {
		t.Fatal(err)
	}
	if info.Flags&unix.UF_HIDDEN != 0 {
		t.Fatal("published file remains hidden")
	}
	raw, err := os.ReadFile(target)
	if err != nil || string(raw) != "SELECT 1;" {
		t.Fatal("file contents changed", err)
	}
	if err := Prepare(target); err != nil {
		t.Fatal("visible file handling is not idempotent", err)
	}
}
