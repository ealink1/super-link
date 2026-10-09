package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
)

func TestNoteSelectedTypingReplacesReleasedDragAndSelectAll(t *testing.T) {
	for _, reverse := range []bool{false, true} {
		for _, all := range []bool{false, true} {
			t.Run(strings.Join([]string{map[bool]string{false: "forward", true: "reverse"}[reverse], map[bool]string{false: "drag", true: "all"}[all]}, "/"), func(t *testing.T) {
				_, n := noteTestWindow(t)
				text := "前缀唯一 " + strings.Repeat("沙发飘风 ", 20) + "\n我是你爸爸 发达到了"
				e := n.editor
				e.SetText(text)
				e.Resize(fyne.NewSize(420, 300))
				e.Refresh()
				start, end := 0, len([]rune(text))
				if !all {
					start, end = 6, 25
					if reverse {
						start, end = end, start
					}
					p := e.richRenderer.nearestOffset(start)
					q := e.richRenderer.nearestOffset(end)
					e.MouseDown(&desktop.MouseEvent{PointEvent: fyne.PointEvent{Position: p.position.Add(fyne.NewPos(0, p.height/2))}, Button: desktop.MouseButtonPrimary})
					e.Dragged(&fyne.DragEvent{PointEvent: fyne.PointEvent{Position: q.position.Add(fyne.NewPos(0, q.height/2))}})
					e.DragEnd()
					e.MouseUp(&desktop.MouseEvent{Button: desktop.MouseButtonPrimary})
				} else {
					e.TypedShortcut(&fyne.ShortcutSelectAll{})
				}
				selected := e.SelectedText()
				if selected == "" {
					t.Fatal("missing selection")
				}
				before := e.Text
				e.KeyDown(&fyne.KeyEvent{Name: fyne.KeyS})
				e.TypedKey(&fyne.KeyEvent{Name: fyne.KeyS})
				e.TypedRune('新')
				e.TypedRune('字')
				expected := string([]rune(before)[:min(start, end)]) + "新字" + string([]rune(before)[max(start, end):])
				if e.Text != expected || e.SelectedText() != "" {
					t.Fatalf("replacement failed: %q / %q selected=%q", e.Text, expected, e.SelectedText())
				}
			})
		}
	}
}
