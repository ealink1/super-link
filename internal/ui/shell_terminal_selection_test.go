package ui

import (
	"fmt"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
)

func selectionTerminal(t *testing.T) *terminalSurface {
	t.Helper()
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	terminal := newTerminalSurface(nil)
	window := app.NewWindow("selection")
	window.SetContent(terminal)
	cell := terminal.cellSize()
	terminal.Resize(fyne.NewSize(cell.Width*20+.1, cell.Height*6+.1))
	terminal.emulator.Resize(20, 6)
	t.Cleanup(func() { terminal.release(); window.Close() })
	return terminal
}
func selectionPosition(t *terminalSurface, x, y int) fyne.Position {
	cell := t.cellSize()
	return fyne.NewPos((float32(x)+.5)*cell.Width, (float32(y)+.5)*cell.Height)
}
func selectionPress(t *terminalSurface, x, y int, modifier fyne.KeyModifier) {
	t.MouseDown(&desktop.MouseEvent{PointEvent: fyne.PointEvent{Position: selectionPosition(t, x, y)}, Button: desktop.MouseButtonPrimary, Modifier: modifier})
}
func selectionDrag(t *terminalSurface, x, y int) {
	t.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{Position: selectionPosition(t, x, y)}})
}
func TestTerminalDragSelectsAcrossRowsInBothDirectionsAndCopiesOnlySelection(t *testing.T) {
	terminal := selectionTerminal(t)
	terminal.feed([]byte("prefix RED\r\nGREEN suffix"))
	for _, reverse := range []bool{false, true} {
		if reverse {
			selectionPress(terminal, 4, 1, 0)
			selectionDrag(terminal, 7, 0)
		} else {
			selectionPress(terminal, 7, 0, 0)
			selectionDrag(terminal, 4, 1)
		}
		terminal.DragEnd()
		// Native Fyne delivers DragEnd before MouseUp; the selection must survive both.
		terminal.MouseUp(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary})
		if got := terminal.selectedText(); got != "RED\nGREEN" {
			t.Fatalf("reverse=%v: %q", reverse, got)
		}
		if terminal.selectedCell(6, 0) || !terminal.selectedCell(7, 0) || !terminal.selectedCell(4, 1) || terminal.selectedCell(5, 1) {
			t.Fatal("selection extends past its exact boundaries")
		}
		clipboard := fyne.CurrentApp().Clipboard()
		terminal.TypedShortcut(&fyne.ShortcutCopy{Clipboard: clipboard})
		if got := clipboard.Content(); got != "RED\nGREEN" {
			t.Fatalf("copied whole terminal instead of selection: %q", got)
		}
		terminal.TypedShortcut(&desktop.CustomShortcut{KeyName: fyne.KeyC, Modifier: fyne.KeyModifierControl | fyne.KeyModifierShift})
		if clipboard.Content() != "RED\nGREEN" {
			t.Fatal("Ctrl+Shift+C did not copy selection")
		}
	}
	selectionPress(terminal, 0, 0, 0)
	terminal.MouseUp(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary})
	if terminal.selection.selected {
		t.Fatal("plain click did not clear old selection")
	}
	if !strings.Contains(terminal.copyText(), "prefix RED") {
		t.Fatal("copy without selection no longer preserves existing behavior")
	}
}
func TestTerminalSelectionPreservesWideAndCombinedCharactersAndANSI(t *testing.T) {
	terminal := selectionTerminal(t)
	terminal.feed([]byte("a\x1b[31m中文\x1b[0me\u0301 z"))
	// Start inside the second display column of 中 and finish at a combining cluster.
	selectionPress(terminal, 2, 0, 0)
	selectionDrag(terminal, 5, 0)
	terminal.DragEnd()
	if got := terminal.selectedText(); got != "中文e\u0301" {
		t.Fatalf("wide/combined glyph split or ANSI copied: %q", got)
	}
	renderer := test.WidgetRenderer(terminal).(*terminalRenderer)
	found := false
	for i, text := range renderer.pool {
		if text.Visible() && strings.Contains(text.Text, "中文") {
			found = true
			if terminalNRGBA(renderer.backgrounds[i].FillColor) != terminalNRGBA(theme.SelectionColor()) {
				t.Fatal("selection has no highlight background")
			}
		}
	}
	if !found {
		t.Fatal("selection glyph run not painted")
	}
	terminal.feed([]byte("fresh"))
	if terminal.selection.selected {
		t.Fatal("changed output retained a stale selection")
	}
}
func TestTerminalSelectionAutoScrollsHistoryAndStopsOnReleaseOrDispose(t *testing.T) {
	terminal := selectionTerminal(t)
	var history strings.Builder
	for i := range 30 {
		fmt.Fprintf(&history, "line-%02d\r\n", i)
	}
	terminal.feed([]byte(history.String()))
	selectionPress(terminal, 0, 0, 0)
	terminal.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{Position: fyne.NewPos(10, -10)}})
	if terminal.selection.animation == nil {
		t.Fatal("drag above viewport did not start auto-scroll")
	}
	terminal.stepSelectionScroll()
	if terminal.scroll != 1 || terminal.selection.head.y >= terminal.selection.anchor.y {
		t.Fatal("auto-scroll did not extend selection into history")
	}
	terminal.MouseUp(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary})
	if terminal.selection.animation != nil || terminal.selection.down {
		t.Fatal("release left auto-scroll running")
	}
	before := terminal.scroll
	terminal.stepSelectionScroll()
	if terminal.scroll != before {
		t.Fatal("scroll continued after release")
	}
	terminal.scroll = 3
	selectionPress(terminal, 0, 0, 0)
	terminal.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{Position: fyne.NewPos(10, terminal.Size().Height+10)}})
	terminal.stepSelectionScroll()
	if terminal.scroll != 2 {
		t.Fatal("drag below viewport did not scroll down")
	}
	if !strings.Contains(terminal.selectedText(), "line-") {
		t.Fatal("history selection contains no text")
	}
	terminal.dispose()
	if terminal.selection.animation != nil {
		t.Fatal("disposed terminal retained auto-scroll animation")
	}
	terminal.Dragged(&fyne.DragEvent{})
	terminal.DragEnd()
}
func TestTerminalVimMouseDragAndShiftLocalSelection(t *testing.T) {
	terminal := selectionTerminal(t)
	emulator := terminal.emulator
	input := make(chan string, 8)
	done := make(chan struct{})
	go func() {
		defer close(done)
		buffer := make([]byte, 128)
		for {
			n, err := emulator.Read(buffer)
			if n > 0 {
				input <- string(buffer[:n])
			}
			if err != nil {
				return
			}
		}
	}()
	t.Cleanup(func() { terminal.dispose(); <-done })
	terminal.feed([]byte("vim text\x1b[?1002;1006h"))
	selectionPress(terminal, 1, 0, 0)
	terminal.feed([]byte("\rVim text")) // Vim can redraw immediately after button-down.
	selectionDrag(terminal, 4, 0)
	terminal.DragEnd()
	var wire strings.Builder
	for range 3 {
		wire.WriteString(<-input)
	}
	if got := wire.String(); got != "\x1b[<0;2;1M\x1b[<32;5;1M\x1b[<0;5;1m" {
		t.Fatalf("remote mouse sequence: %q", got)
	}
	if terminal.selection.selected {
		t.Fatal("Vim drag unexpectedly selected local text")
	}
	selectionPress(terminal, 0, 0, fyne.KeyModifierShift)
	selectionDrag(terminal, 2, 0)
	terminal.DragEnd()
	if got := terminal.selectedText(); got != "Vim" {
		t.Fatalf("Shift did not override application mouse mode: %q", got)
	}
	select {
	case data := <-input:
		t.Fatalf("local selection leaked input: %q", data)
	default:
	}
}
