//go:build darwin && cgo

package ui

/*
#cgo CFLAGS: -fblocks
#cgo LDFLAGS: -framework AppKit -framework Foundation
#include <stdint.h>
int superlink_lifecycle_install(uintptr_t callback);
void superlink_lifecycle_remove(uintptr_t callback);
*/
import "C"

import (
	"runtime/cgo"
	"sync"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver"
)

type applicationLifecycleActions struct {
	restore, quit func()
}

//export superlinkApplicationAction
func superlinkApplicationAction(callback C.uintptr_t, quit C.int) {
	actions := cgo.Handle(callback).Value().(applicationLifecycleActions)
	action := actions.restore
	if quit != 0 {
		action = actions.quit
	}
	fyne.Do(action)
}

func installApplicationLifecycle(window fyne.Window, restore, quit func()) func() {
	native, ok := window.(driver.NativeWindow)
	if !ok {
		return nil
	}
	handle := cgo.NewHandle(applicationLifecycleActions{restore: restore, quit: quit})
	installed := false
	native.RunNative(func(context any) {
		if context, ok := context.(driver.MacWindowContext); ok && context.NSWindow != 0 {
			installed = C.superlink_lifecycle_install(C.uintptr_t(handle)) != 0
		}
	})
	if !installed {
		handle.Delete()
		return nil
	}
	var once sync.Once
	return func() {
		once.Do(func() {
			C.superlink_lifecycle_remove(C.uintptr_t(handle))
			handle.Delete()
		})
	}
}
