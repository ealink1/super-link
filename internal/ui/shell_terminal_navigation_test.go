package ui

import (
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
)

func TestTerminalHomeEndModesAndModifiers(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	terminal := newTerminalSurface(nil)
	input := make(chan string, 32)
	done := make(chan struct{})
	go func() {
		defer close(done)
		buffer := make([]byte, 128)
		for {
			n, err := terminal.emulator.Read(buffer)
			if n > 0 {
				input <- string(buffer[:n])
			}
			if err != nil {
				return
			}
		}
	}()
	defer func() { terminal.dispose(); <-done }()
	cases := []struct {
		mode string
		key  fyne.KeyName
		mod  fyne.KeyModifier
		want string
	}{
		{"", fyne.KeyHome, 0, "\x1b[H"},
		{"", fyne.KeyEnd, 0, "\x1b[F"},
		{"\x1b[?1h", fyne.KeyHome, 0, "\x1bOH"},
		{"", fyne.KeyEnd, 0, "\x1bOF"},
		{"", fyne.KeyHome, fyne.KeyModifierShift, "\x1b[1;2H"},
		{"", fyne.KeyEnd, fyne.KeyModifierControl, "\x1b[1;5F"},
		{"", fyne.KeyHome, fyne.KeyModifierAlt, "\x1b[1;3H"},
		{"", fyne.KeyEnd, fyne.KeyModifierShift | fyne.KeyModifierControl, "\x1b[1;6F"},
		{"\x1b[?1l", fyne.KeyHome, 0, "\x1b[H"},
		{"", fyne.KeyEnd, 0, "\x1b[F"},
	}
	for _, tc := range cases {
		terminal.feed([]byte(tc.mode))
		terminal.scroll = 3
		if tc.mod == 0 {
			terminal.TypedKey(&fyne.KeyEvent{Name: tc.key})
		} else {
			terminal.TypedShortcut(&desktop.CustomShortcut{KeyName: tc.key, Modifier: tc.mod})
		}
		select {
		case got := <-input:
			if got != tc.want {
				t.Fatalf("%s mod %d: got %q, want %q", tc.key, tc.mod, got, tc.want)
			}
		case <-time.After(time.Second):
			t.Fatalf("%s mod %d produced no input", tc.key, tc.mod)
		}
		if terminal.scroll != 0 {
			t.Fatal("navigation did not return to live terminal")
		}
	}
}
