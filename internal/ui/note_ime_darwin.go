//go:build darwin && cgo

package ui

/*
#cgo LDFLAGS: -framework AppKit -framework Foundation
#include <stdint.h>
void superlink_note_ime(uintptr_t window, double x, double y, double height, double width, double canvasHeight, int active);
*/
import "C"

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver"
)

func nativeInputMethodsEnabled() bool { return true }
func publishInputMethod(window fyne.Window, origin fyne.Position, caret fyne.Size, enabled bool) {
	native, ok := window.(driver.NativeWindow)
	if !ok {
		return
	}
	size := window.Canvas().Size()
	active := C.int(0)
	if enabled {
		active = 1
	}
	native.RunNative(func(context any) {
		if mac, ok := context.(driver.MacWindowContext); ok {
			C.superlink_note_ime(C.uintptr_t(mac.NSWindow), C.double(origin.X), C.double(origin.Y), C.double(caret.Height), C.double(size.Width), C.double(size.Height), active)
		}
	})
}
