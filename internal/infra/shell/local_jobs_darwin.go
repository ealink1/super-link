//go:build darwin

package shell

import "syscall"

// Darwin's PTY hangup terminates background process groups as well.
func signalLocalJobs(_ int, _ syscall.Signal) error { return nil }
