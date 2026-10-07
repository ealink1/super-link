//go:build darwin || linux

package shell

import (
	"context"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
)

type localSession struct {
	file *os.File
	cmd  *exec.Cmd
	done chan struct{}
	once sync.Once
	// pty.Setsize uses File.Fd, which must not race with File.Close.
	fdMu   sync.Mutex
	closed bool
}

// OpenLocal starts a login shell in a real pseudo-terminal.
func OpenLocal(ctx context.Context, executable string) (Session, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if executable == "" {
		executable = os.Getenv("SHELL")
	}
	if executable == "" {
		executable = "/bin/sh"
	}
	if !filepath.IsAbs(executable) {
		return nil, errors.New("本地壳必须是可执行文件的绝对路径")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, err
	}
	cmd := exec.Command(executable, "-l")
	cmd.Dir = home
	cmd.Env = append(os.Environ(), "TERM=xterm-256color", "COLORTERM=truecolor")
	file, err := pty.StartWithSize(cmd, &pty.Winsize{Rows: 24, Cols: 80})
	if err != nil {
		return nil, err
	}
	s := &localSession{file: file, cmd: cmd, done: make(chan struct{})}
	go func() {
		defer close(s.done)
		_ = cmd.Wait() // Exit status is reflected by PTY EOF; no command output is logged.
	}()
	if err := ctx.Err(); err != nil {
		return nil, errors.Join(err, s.Close())
	}
	return s, nil
}

func (s *localSession) Read(p []byte) (int, error) {
	n, err := s.file.Read(p)
	if errors.Is(err, syscall.EIO) {
		err = io.EOF
	}
	return n, err
}
func (s *localSession) Write(p []byte) (int, error) { return s.file.Write(p) }
func (s *localSession) Resize(cols, rows int) error {
	s.fdMu.Lock()
	defer s.fdMu.Unlock()
	if s.closed {
		return os.ErrClosed
	}
	return pty.Setsize(s.file, &pty.Winsize{Cols: uint16(min(max(cols, 2), 500)), Rows: uint16(min(max(rows, 2), 200))})
}
func (s *localSession) Close() error {
	var err error
	s.once.Do(func() {
		err = errors.Join(err, signalLocalJobs(s.cmd.Process.Pid, syscall.SIGTERM))
		var signalErr error
		// pty.Start creates a distinct session/process group. Terminate children too.
		select {
		case <-s.done:
		default:
			signalErr = syscall.Kill(-s.cmd.Process.Pid, syscall.SIGTERM)
			if errors.Is(signalErr, syscall.ESRCH) {
				signalErr = nil
			}
		}
		s.fdMu.Lock()
		s.closed = true
		err = errors.Join(err, s.file.Close())
		s.fdMu.Unlock()
		timer := time.NewTimer(2 * time.Second)
		defer timer.Stop()
		select {
		case <-s.done:
			// Darwin can return EPERM for a process group whose final member
			// exited between PTY EOF and Wait. Only ignore it after joining.
			if !errors.Is(signalErr, syscall.EPERM) {
				err = errors.Join(err, signalErr)
			}
		case <-timer.C:
			err = errors.Join(err, signalErr)
			killErr := syscall.Kill(-s.cmd.Process.Pid, syscall.SIGKILL)
			if !errors.Is(killErr, syscall.ESRCH) {
				err = errors.Join(err, killErr)
			}
			<-s.done
		}
		// A job can ignore TERM even when its parent shell already exited.
		err = errors.Join(err, signalLocalJobs(s.cmd.Process.Pid, syscall.SIGKILL))
	})
	return err
}
