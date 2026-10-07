package state

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
)

func TestBackupIncludesWALAndIsIndependent(t *testing.T) {
	ctx := context.Background()
	root := t.TempDir()
	store, err := Open(filepath.Join(root, "state.sqlite"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	p, err := store.SaveProfile(ctx, domain.Profile{ID: "p", Name: "before"})
	if err != nil {
		t.Fatal(err)
	}
	backup := filepath.Join(root, "备份 ' #.sqlite")
	if err = store.BackupTo(ctx, backup); err != nil {
		t.Fatal(err)
	}
	p.Name = "after"
	if _, err = store.SaveProfile(ctx, p); err != nil {
		t.Fatal(err)
	}
	if err = store.BackupTo(ctx, backup); err == nil {
		t.Fatal("overwrote a snapshot")
	}
	snapshot, err := Open(backup)
	if err != nil {
		t.Fatal(err)
	}
	defer snapshot.Close()
	previous, err := snapshot.Profile(ctx, "p")
	if err != nil || previous.Name != "before" {
		t.Fatalf("inconsistent snapshot: %#v %v", previous, err)
	}
	info, err := os.Stat(backup)
	if err != nil {
		t.Fatal(err)
	}
	// Windows reports writability, not the inherited user-directory ACL,
	// through FileMode. POSIX hosts must retain owner-only permissions.
	if !info.Mode().IsRegular() || info.Mode().Perm()&0200 == 0 || (runtime.GOOS != "windows" && info.Mode().Perm() != 0600) {
		t.Fatal("backup permissions")
	}
}
