//go:build !darwin || !cgo

package ui

import "fyne.io/fyne/v2"

func nativeInputMethodsEnabled() bool                                { return false }
func publishInputMethod(fyne.Window, fyne.Position, fyne.Size, bool) {}
