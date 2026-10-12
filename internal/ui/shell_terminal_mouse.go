package ui

import (
	"fyne.io/fyne/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/vt"
)

// Track only the five mouse reporting modes; arbitrary server modes cannot
// grow this state. The emulator owns the wire encoding (SGR or legacy X10).
func (t *terminalSurface) setMouseMode(mode ansi.Mode, enabled bool) {
	switch mode {
	case ansi.ModeMouseX10, ansi.ModeMouseNormal, ansi.ModeMouseHighlight, ansi.ModeMouseButtonEvent, ansi.ModeMouseAnyEvent:
	default:
		return
	}
	if enabled {
		if t.mouseModes == nil {
			t.mouseModes = make(map[ansi.Mode]bool, 5)
		}
		t.mouseModes[mode] = true
	} else {
		delete(t.mouseModes, mode)
	}
}

func (t *terminalSurface) scrollRemote(event *fyne.ScrollEvent) bool {
	modifiers := terminalDesktopModifiers()
	if len(t.mouseModes) > 0 && modifiers&fyne.KeyModifierShift == 0 {
		button := uv.MouseWheelDown
		if event.Scrolled.DY > 0 {
			button = uv.MouseWheelUp
		}
		cell := t.cellSize()
		x := min(t.emulator.Width()-1, max(0, int(event.Position.X/cell.Width)))
		y := min(t.emulator.Height()-1, max(0, int(event.Position.Y/cell.Height)))
		t.scroll = 0
		t.emulator.SendMouse(uv.MouseWheelEvent{X: x, Y: y, Button: button, Mod: terminalModifiers(modifiers)})
		return true
	}
	if !t.altScreen {
		return false
	}
	// Vim with mouse reporting disabled still accepts the usual alternate-screen
	// wheel-to-cursor-key behavior, without scrolling stale shell history.
	if modifiers&fyne.KeyModifierShift == 0 {
		key := vt.KeyDown
		if event.Scrolled.DY > 0 {
			key = vt.KeyUp
		}
		for range 3 {
			t.emulator.SendKey(vt.KeyPressEvent{Code: key, Mod: terminalModifiers(modifiers)})
		}
	}
	return true
}
