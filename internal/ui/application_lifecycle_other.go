//go:build !darwin || !cgo

package ui

import "fyne.io/fyne/v2"

func installApplicationLifecycle(fyne.Window, func(), func()) func() { return nil }
