package ui

import (
	"strings"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
)

func TestTerminalWheelReachesVimAndReturnsToShellHistory(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
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
	terminal.Resize(fyne.NewSize(480, 240))
	cell := terminal.cellSize()
	wheel := &fyne.ScrollEvent{PointEvent: fyne.PointEvent{Position: fyne.NewPos(cell.Width*4.5, cell.Height*2.5)}, Scrolled: fyne.Delta{DY: 1}}
	expect := func(want string) {
		t.Helper()
		var got strings.Builder
		deadline := time.NewTimer(time.Second)
		defer deadline.Stop()
		for got.Len() < len(want) {
			select {
			case data := <-input:
				got.WriteString(data)
			case <-deadline.C:
				t.Fatalf("wheel input: got %q, want %q", got.String(), want)
			}
		}
		if got.String() != want {
			t.Fatalf("wheel input: got %q, want %q", got.String(), want)
		}
	}
	terminal.feed([]byte("\x1b[?1049h\x1b[?1000;1006h"))
	terminal.Scrolled(wheel)
	expect("\x1b[<64;5;3M")
	wheel.Scrolled.DY = -1
	terminal.Scrolled(wheel)
	expect("\x1b[<65;5;3M")
	if terminal.scroll != 0 {
		t.Fatal("Vim wheel scrolled shell history")
	}
	terminal.feed([]byte("\x1b[?1002h\x1b[?1000l\x1b[?1006l"))
	terminal.Scrolled(wheel)
	expect("\x1b[M" + string([]byte{97, 37, 35}))
	terminal.feed([]byte("\x1b[?1002l"))
	terminal.Scrolled(wheel)
	expect(strings.Repeat("\x1b[B", 3))
	wheel.Scrolled.DY = 1
	terminal.Scrolled(wheel)
	expect(strings.Repeat("\x1b[A", 3))
	terminal.feed([]byte("\x1b[?1049l" + strings.Repeat("history\r\n", 50)))
	terminal.Scrolled(wheel)
	if terminal.scroll != 3 {
		t.Fatalf("shell history did not resume: %d", terminal.scroll)
	}
	wheel.Scrolled.DY = 0
	terminal.Scrolled(wheel)
	if terminal.scroll != 3 {
		t.Fatal("horizontal-only scroll changed history")
	}
	select {
	case data := <-input:
		t.Fatalf("shell wheel leaked remote input %q", data)
	default:
	}
	terminal.dispose()
	terminal.Scrolled(wheel)
}
