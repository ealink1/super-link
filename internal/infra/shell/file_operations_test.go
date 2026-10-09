package shell

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFileOperationsRefuseInvalidPathsAndCancellation(t *testing.T) {
	r := &Remote{}
	for _, filename := range []string{"", "/", ".", "..", "/a/..", "x\x00y"} {
		if r.RemoveFile(context.Background(), filename) == nil {
			t.Fatal("unsafe path", filename)
		}
	}
	if r.SetFilePermissions(context.Background(), "/file", 01000) == nil {
		t.Fatal("special permissions accepted")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if !errors.Is(r.RemoveFile(ctx, "/file"), context.Canceled) {
		t.Fatal("cancellation ignored")
	}
}
func TestSFTPFileOperationsProtectTargets(t *testing.T) {
	fixture := newSSHFixture(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	remote, err := OpenSSH(ctx, fixture.host)
	if err != nil {
		t.Fatal(err)
	}
	defer remote.Close()
	source := filepath.Join(t.TempDir(), "source")
	if err := os.WriteFile(source, []byte("fixture"), 0600); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"/one", "/two"} {
		if err := remote.Upload(ctx, source, name, nil); err != nil {
			t.Fatal(err)
		}
	}
	if err := remote.RenameFile(ctx, "/one", "/two"); err == nil {
		t.Fatal("rename overwrote target")
	}
	if err := remote.RenameFile(ctx, "/one", "/renamed"); err != nil {
		t.Fatal(err)
	}
	if err := remote.SetFilePermissions(ctx, "/renamed", 0640); err != nil {
		t.Fatal(err)
	}
	if err := remote.RemoveFile(ctx, "/renamed"); err != nil {
		t.Fatal(err)
	}
	files, err := remote.Files(ctx, "/")
	if err != nil || len(files) != 1 || files[0].Name != "two" {
		t.Fatal(files, err)
	}
}

func TestSFTPAbsolutePath(t *testing.T) {
	fixture := newSSHFixture(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	remote, err := OpenSSH(ctx, fixture.host)
	if err != nil {
		t.Fatal(err)
	}
	defer remote.Close()
	for input, want := range map[string]string{".": "/", "ott": "/ott", "/ott/../client": "/client"} {
		got, err := remote.AbsolutePath(ctx, input)
		if err != nil || got != want {
			t.Fatalf("resolve %q: got %q, want %q, err %v", input, got, want, err)
		}
	}
	for _, input := range []string{"", "bad\x00path"} {
		if _, err := remote.AbsolutePath(ctx, input); err == nil {
			t.Fatalf("accepted invalid path %q", input)
		}
	}
	cancel()
	if _, err := remote.AbsolutePath(ctx, "."); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancellation ignored: %v", err)
	}
}
