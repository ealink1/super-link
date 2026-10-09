//go:build !darwin

package filevisibility

func Prepare(string) error { return nil }
