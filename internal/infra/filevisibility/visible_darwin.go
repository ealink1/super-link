//go:build darwin

// Package filevisibility clears inherited hidden metadata before publishing files.
package filevisibility

import "golang.org/x/sys/unix"

func Prepare(path string) error {
	var info unix.Stat_t
	if err := unix.Stat(path, &info); err != nil {
		return err
	}
	if info.Flags&unix.UF_HIDDEN == 0 {
		return nil
	}
	return unix.Chflags(path, int(info.Flags&^unix.UF_HIDDEN))
}
