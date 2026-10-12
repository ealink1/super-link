package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/vt"
)

func (t *terminalSurface) TypedRune(value rune) {
	if !t.closed {
		t.scroll = 0
		t.emulator.SendText(string(value))
	}
}

var terminalKeys = map[fyne.KeyName]rune{
	fyne.KeyReturn: vt.KeyEnter, fyne.KeyEnter: vt.KeyEnter,
	fyne.KeyBackspace: vt.KeyBackspace, fyne.KeyEscape: vt.KeyEscape, fyne.KeyTab: vt.KeyTab,
	fyne.KeyUp: vt.KeyUp, fyne.KeyDown: vt.KeyDown, fyne.KeyLeft: vt.KeyLeft, fyne.KeyRight: vt.KeyRight,
	fyne.KeyHome: vt.KeyHome, fyne.KeyEnd: vt.KeyEnd, fyne.KeyDelete: vt.KeyDelete, fyne.KeyInsert: vt.KeyInsert,
	fyne.KeyPageUp: vt.KeyPgUp, fyne.KeyPageDown: vt.KeyPgDown,
	fyne.KeyF1: vt.KeyF1, fyne.KeyF2: vt.KeyF2, fyne.KeyF3: vt.KeyF3, fyne.KeyF4: vt.KeyF4,
	fyne.KeyF5: vt.KeyF5, fyne.KeyF6: vt.KeyF6, fyne.KeyF7: vt.KeyF7, fyne.KeyF8: vt.KeyF8,
	fyne.KeyF9: vt.KeyF9, fyne.KeyF10: vt.KeyF10, fyne.KeyF11: vt.KeyF11, fyne.KeyF12: vt.KeyF12,
}

func (t *terminalSurface) TypedKey(key *fyne.KeyEvent) {
	if code, ok := terminalKeys[key.Name]; ok && !t.closed {
		t.scroll = 0
		t.sendTerminalKey(code, terminalDesktopModifiers())
	}
}

func (t *terminalSurface) TypedShortcut(shortcut fyne.Shortcut) {
	if t.closed {
		return
	}
	if terminalDesktopModifiers()&fyne.KeyModifierSuper != 0 {
		switch shortcut.(type) {
		case *fyne.ShortcutCut, *fyne.ShortcutSelectAll, *fyne.ShortcutUndo, *fyne.ShortcutRedo:
			return
		}
	}
	switch shortcut := shortcut.(type) {
	case *fyne.ShortcutPaste:
		if terminalDesktopModifiers() == fyne.KeyModifierControl {
			t.sendKey(fyne.KeyV, fyne.KeyModifierControl)
			return
		}
		text := shortcut.Clipboard.Content()
		if len(text) <= 64<<10 {
			t.scroll = 0
			t.emulator.Paste(text)
		} else if t.rejectPaste != nil {
			t.rejectPaste()
		}
	case *fyne.ShortcutCopy:
		if terminalDesktopModifiers() == fyne.KeyModifierControl {
			t.sendKey(fyne.KeyC, fyne.KeyModifierControl)
			return
		}
		shortcut.Clipboard.SetContent(t.emulator.String())
	case *fyne.ShortcutCut:
		t.sendKey(fyne.KeyX, fyne.KeyModifierControl)
	case *fyne.ShortcutSelectAll:
		t.sendKey(fyne.KeyA, fyne.KeyModifierControl)
	case *fyne.ShortcutUndo:
		t.sendKey(fyne.KeyZ, fyne.KeyModifierControl)
	case *fyne.ShortcutRedo:
		t.sendKey(fyne.KeyY, fyne.KeyModifierControl)
	case fyne.KeyboardShortcut:
		if shortcut.Mod() == fyne.KeyModifierControl|fyne.KeyModifierShift && shortcut.Key() == fyne.KeyC {
			fyne.CurrentApp().Clipboard().SetContent(t.emulator.String())
			return
		}
		if shortcut.Mod() == fyne.KeyModifierControl|fyne.KeyModifierShift && shortcut.Key() == fyne.KeyV {
			t.TypedShortcut(&fyne.ShortcutPaste{Clipboard: fyne.CurrentApp().Clipboard()})
			return
		}
		t.sendKey(shortcut.Key(), shortcut.Mod())
	}
}

func terminalDesktopModifiers() fyne.KeyModifier {
	if driver, ok := fyne.CurrentApp().Driver().(desktop.Driver); ok {
		return driver.CurrentKeyModifiers()
	}
	return 0
}
func terminalModifiers(mod fyne.KeyModifier) vt.KeyMod {
	var result vt.KeyMod
	if mod&fyne.KeyModifierControl != 0 {
		result |= vt.ModCtrl
	}
	if mod&fyne.KeyModifierAlt != 0 {
		result |= vt.ModAlt
	}
	if mod&fyne.KeyModifierShift != 0 {
		result |= vt.ModShift
	}
	return result
}
func (t *terminalSurface) sendKey(key fyne.KeyName, mod fyne.KeyModifier) {
	if mod&fyne.KeyModifierSuper != 0 {
		return
	}
	code, ok := terminalKeys[key]
	if !ok {
		name := strings.ToLower(string(key))
		switch {
		case key == fyne.KeySpace:
			code = ' '
		case len(name) == 1:
			code = rune(name[0])
		default:
			return
		}
	}
	t.scroll = 0
	t.sendTerminalKey(code, mod)
}

func (t *terminalSurface) setInputMode(mode ansi.Mode, enabled bool) {
	if mode == ansi.DECMode(1) {
		t.applicationCursor = enabled
	}
	t.setMouseMode(mode, enabled)
}

// xterm Home/End follow DECCKM, just like the arrow keys. The emulator's
// encoder currently ignores this mode for these two keys and drops modifiers.
func (t *terminalSurface) sendTerminalKey(code rune, mod fyne.KeyModifier) {
	if code != vt.KeyHome && code != vt.KeyEnd {
		t.emulator.SendKey(vt.KeyPressEvent{Code: code, Mod: terminalModifiers(mod)})
		return
	}
	suffix := "H"
	if code == vt.KeyEnd {
		suffix = "F"
	}
	parameter := 1
	if mod&fyne.KeyModifierShift != 0 {
		parameter += 1
	}
	if mod&fyne.KeyModifierAlt != 0 {
		parameter += 2
	}
	if mod&fyne.KeyModifierControl != 0 {
		parameter += 4
	}
	sequence := "\x1b[" + suffix
	if parameter != 1 {
		sequence = fmt.Sprintf("\x1b[1;%d%s", parameter, suffix)
	} else if t.applicationCursor {
		sequence = "\x1bO" + suffix
	}
	t.emulator.SendText(sequence)
}
