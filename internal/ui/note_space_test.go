package ui

import (
	"fyne.io/fyne/v2"
	"testing"
)

func TestNoteSpacesAdvanceCaret(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	for _, value := range []string{"", "正文", "  正文", "**正文**"} {
		n.editor.SetText(value)
		n.editor.Resize(fyne.NewSize(600, 300))
		end := len([]rune(value))
		n.editor.selectSource(end, end)
		n.editor.Refresh()
		previous := n.editor.richRenderer.nearestOffset(end).position
		for i := 1; i <= 4; i++ {
			n.editor.TypedRune(' ')
			n.editor.Refresh()
			next := n.editor.richRenderer.nearestOffset(end + i).position
			if next.X <= previous.X || next.Y != previous.Y {
				t.Fatalf("space did not advance caret for %q: %v -> %v", value, previous, next)
			}
			previous = next
		}
		n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
		n.editor.Refresh()
		if n.editor.richRenderer.nearestOffset(end+3).position.X >= previous.X {
			t.Fatal("space deletion did not move caret back")
		}
	}
}
func TestNoteWhitespaceRunsPreserveOffsets(t *testing.T) {
	runs := noteInlineRuns("  **正文**   ", 10, 16, fyne.TextStyle{})
	if len(runs) != 3 || runs[0].text != "  " || runs[0].start != 10 || runs[1].start != 14 || !runs[1].style.Bold || runs[2].text != "   " || runs[2].start != 18 {
		t.Fatalf("whitespace source mapping: %#v", runs)
	}
}
