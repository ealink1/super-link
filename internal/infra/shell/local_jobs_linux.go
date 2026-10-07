//go:build linux

package shell

import (
	"errors"
	"io"
	"os"
	"strconv"
	"syscall"

	"golang.org/x/sys/unix"
)

// Interactive job control gives background jobs their own process groups.
// They still share the PTY's session, even after the shell exits. Enumerate
// process IDs in bounded batches and validate session ownership before signals.
func signalLocalJobs(sessionID int, signal syscall.Signal) error {
	processes, err := os.Open("/proc")
	if err != nil {
		return err
	}
	defer processes.Close()
	var signalErr error
	for {
		entries, readErr := processes.ReadDir(128)
		for _, entry := range entries {
			pid, err := strconv.Atoi(entry.Name())
			if err != nil || pid == sessionID {
				continue
			}
			sid, err := unix.Getsid(pid)
			if err != nil || sid != sessionID {
				continue
			}
			if err := syscall.Kill(pid, signal); err != nil && !errors.Is(err, syscall.ESRCH) {
				signalErr = errors.Join(signalErr, err)
			}
		}
		if errors.Is(readErr, io.EOF) {
			return signalErr
		}
		if readErr != nil {
			return errors.Join(signalErr, readErr)
		}
	}
}
