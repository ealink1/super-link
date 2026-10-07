//go:build darwin || linux

package shell

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

func TestLocalPTYResizeInteractiveAndClose(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	s, err := OpenLocal(ctx, fixtureLocalShell(t))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	context.AfterFunc(ctx, func() { s.Close() })
	if err := s.Resize(91, 31); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Write([]byte("stty size; printf 'PTY_%s\\n' '中文'; exit\n")); err != nil {
		t.Fatal(err)
	}
	raw, _ := io.ReadAll(io.LimitReader(s, 64<<10))
	if !strings.Contains(string(raw), "31 91") || !strings.Contains(string(raw), "PTY_中文") {
		t.Fatalf("not interactive/resized: %q", raw)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Close(); err != nil {
		t.Fatal("close not idempotent", err)
	}
}

func TestLocalCloseReleasesBackgroundJob(t *testing.T) {
	t.Run("background process group", func(t *testing.T) {
		assertLocalCloseReleasesBackgroundJob(t, "sleep 30 & printf 'CHILD_%s_END\\n' $!\n")
	})
	if runtime.GOOS == "linux" {
		t.Run("job ignoring TERM and HUP", func(t *testing.T) {
			assertLocalCloseReleasesBackgroundJob(t, "sh -c 'trap \"\" TERM HUP; printf \"CHILD_%s_END\\n\" $$; exec sleep 30' &\n")
		})
	}
}

func assertLocalCloseReleasesBackgroundJob(t *testing.T, command string) {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	s, err := OpenLocal(context.Background(), fixtureLocalShell(t))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	context.AfterFunc(ctx, func() { s.Close() })
	_, err = s.Write([]byte(command))
	if err != nil {
		t.Fatal(err)
	}
	pattern := regexp.MustCompile(`CHILD_([0-9]+)_END`)
	buffer := make([]byte, 4096)
	var output string
	var match []string
	for match == nil {
		n, err := s.Read(buffer)
		output += string(buffer[:n])
		match = pattern.FindStringSubmatch(output)
		if err != nil {
			t.Fatalf("read background PID: %v (%q)", err, output)
		}
	}
	pid, _ := strconv.Atoi(match[1])
	defer syscall.Kill(pid, syscall.SIGKILL)
	s.Close()
	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if err := syscall.Kill(pid, 0); err == syscall.ESRCH {
			return
		}
		// Linux CI can leave a terminated orphan as a zombie until PID 1
		// reaps it. kill(pid, 0) alone then incorrectly reports a live job.
		if runtime.GOOS == "linux" {
			raw, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat")
			if err == nil {
				_, fields, ok := strings.Cut(string(raw), ") ")
				if ok && strings.HasPrefix(fields, "Z ") {
					return
				}
			}
		}
		time.Sleep(10 * time.Millisecond)
	}
	stat, _ := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat")
	t.Fatalf("background job survived PTY close: %s", stat)
}

func TestLocalCloseUnblocksRead(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	s, err := OpenLocal(context.Background(), fixtureLocalShell(t))
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan struct{})
	go func() { io.Copy(io.Discard, s); close(done) }()
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("PTY read remained blocked")
	}
}

func TestLocalResizeConcurrentWithClose(t *testing.T) {
	s, err := OpenLocal(context.Background(), fixtureLocalShell(t))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	started := make(chan struct{})
	done := make(chan error, 1)
	go func() {
		close(started)
		for i := 0; i < 1000; i++ {
			if err := s.Resize(80+i%10, 24+i%5); err != nil {
				if errors.Is(err, os.ErrClosed) {
					done <- nil
				} else {
					done <- err
				}
				return
			}
		}
		done <- nil
	}()
	<-started
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-done:
		if err != nil {
			t.Fatal("concurrent resize failed", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("resize remained blocked after close")
	}
	if err := s.Resize(80, 24); !errors.Is(err, os.ErrClosed) {
		t.Fatal("resize after close did not report a closed session", err)
	}
}

func fixtureLocalShell(t *testing.T) string {
	t.Helper()
	t.Setenv("ENV", "")
	filename := filepath.Join(t.TempDir(), "fixture-shell")
	if err := os.WriteFile(filename, []byte("#!/bin/sh\nexec /bin/sh -i\n"), 0700); err != nil {
		t.Fatal(err)
	}
	return filename
}
