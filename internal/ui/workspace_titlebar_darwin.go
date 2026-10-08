//go:build darwin && cgo

package ui

/*
#cgo CFLAGS: -fblocks
#cgo LDFLAGS: -framework AppKit -framework Foundation
#include <stdint.h>
int navi_workspace_install(uintptr_t window, uintptr_t callback);
void navi_workspace_select(uintptr_t window, int mode, int dark, uint32_t background, uint32_t accent);
void navi_workspace_remove(uintptr_t window);
*/
import "C"

import (
	"image/color"
	"runtime/cgo"
	"sync"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver"
	"fyne.io/fyne/v2/theme"
)

type titlebarCallbacks struct {
	workspace func(int)
	utility   func(int)
}

//export naviWorkspaceChanged
func naviWorkspaceChanged(callback C.uintptr_t, mode C.int) {
	run := cgo.Handle(callback).Value().(titlebarCallbacks).workspace
	fyne.Do(func() { run(int(mode)) })
}

//export naviUtilityChanged
func naviUtilityChanged(callback C.uintptr_t, action C.int) {
	run := cgo.Handle(callback).Value().(titlebarCallbacks).utility
	fyne.Do(func() { run(int(action)) })
}

func installWorkspaceTitlebar(window fyne.Window, run func(int), utility func(int)) (func(int, bool), func(), bool) {
	native, ok := window.(driver.NativeWindow)
	if !ok {
		return nil, nil, false
	}
	handle := cgo.NewHandle(titlebarCallbacks{workspace: run, utility: utility})
	var pointer C.uintptr_t
	native.RunNative(func(context any) {
		if context, ok := context.(driver.MacWindowContext); ok {
			pointer = C.uintptr_t(context.NSWindow)
		}
	})
	if pointer == 0 || C.navi_workspace_install(pointer, C.uintptr_t(handle)) == 0 {
		handle.Delete()
		return nil, nil, false
	}
	applyNativeApplicationIcon()
	var once sync.Once
	return func(mode int, dark bool) {
			darkMode := C.int(0)
			if dark {
				darkMode = 1
			}
			selected := Theme{Dark: dark}
			background := nativeThemeRGB(selected.Color(theme.ColorNameBackground, theme.VariantLight))
			accent := nativeThemeRGB(color.NRGBA{R: 51, G: 122, B: 255, A: 255})
			C.navi_workspace_select(pointer, C.int(mode), darkMode, background, accent)
		}, func() {
			once.Do(func() {
				C.navi_workspace_remove(pointer)
				handle.Delete()
			})
		}, true
}

func nativeThemeRGB(shade color.Color) C.uint32_t {
	c := color.NRGBAModel.Convert(shade).(color.NRGBA)
	return C.uint32_t(uint32(c.R)<<16 | uint32(c.G)<<8 | uint32(c.B))
}
