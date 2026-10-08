package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
)

func TestNoteCodeFencesCannotBeJoinedIntoBody(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	original := "```\n内容\n```"
	n.editor.SetText(original)
	n.editor.selectSource(6, 6)
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyDelete})
	if n.editor.Text != original {
		t.Fatalf("closing fence joined into body: %q", n.editor.Text)
	}
	n.editor.selectSource(4, 4)
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	if n.editor.Text != original {
		t.Fatalf("opening fence joined into body: %q", n.editor.Text)
	}
	n.editor.Refresh()
	for _, point := range n.editor.richRenderer.points {
		if point.offset < 4 || point.offset > 6 {
			t.Fatalf("hidden fence is editable at %d", point.offset)
		}
	}
	n.editor.selectSource(6, 6)
	n.editor.TypedRune('新')
	if n.editor.Text != "```\n内容新\n```" {
		t.Fatal(n.editor.Text)
	}
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	if n.editor.Text != original {
		t.Fatalf("normal deletion broken: %q", n.editor.Text)
	}
	var shown strings.Builder
	for _, o := range test.WidgetRenderer(n.editor).Objects() {
		if text, ok := o.(*canvas.Text); ok {
			shown.WriteString(text.Text)
		}
	}
	if shown.String() != "内容" {
		t.Fatalf("syntax leaked: %q", shown.String())
	}
}
func TestNoteEmptyAndMultilineCodeBoundary(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("```\n\n```")
	n.editor.selectSource(4, 4)
	n.editor.TypedRune('甲')
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
	n.editor.TypedRune('乙')
	if n.editor.Text != "```\n甲\n乙\n```" {
		t.Fatal(n.editor.Text)
	}
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	if n.editor.Text != "```\n甲\n```" {
		t.Fatalf("code line join broken: %q", n.editor.Text)
	}
}

func TestNoteEmptyCodeMouseInputToggleAndDelete(t *testing.T) {
	for _, source := range []string{"```\n```", "```go\n```", "```\n\n```"} {
		t.Run(source, func(t *testing.T) {
			w, n := noteTestWindow(t)
			w.Window.SetContent(n.content)
			n.newNote()
			n.editor.SetText(source)
			n.editor.Resize(fyne.NewSize(500, 200))
			n.editor.Refresh()
			r := n.editor.richRenderer
			if len(r.points) == 0 {
				t.Fatal("empty code has no editable position")
			}
			point := r.points[0]
			n.editor.MouseDown(&desktop.MouseEvent{PointEvent: fyne.PointEvent{Position: fyne.NewPos(150, point.position.Y+point.height/2)}, Button: desktop.MouseButtonPrimary})
			n.editor.TypedRune('字')
			if !strings.Contains(n.editor.Text, "\n字\n") {
				t.Fatalf("mouse input entered fence: %q", n.editor.Text)
			}
			n.editor.Undo()
			normalized := n.editor.Text
			n.editor.MouseDown(&desktop.MouseEvent{PointEvent: fyne.PointEvent{Position: point.position}, Button: desktop.MouseButtonPrimary})
			n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
			if n.editor.Text != "" {
				t.Fatalf("empty code cannot be deleted: %q", n.editor.Text)
			}
			n.editor.Undo()
			if n.editor.Text != normalized {
				t.Fatal("undo did not restore empty code")
			}

		})
	}
}
