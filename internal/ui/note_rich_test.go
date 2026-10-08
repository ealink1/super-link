package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
)

func TestNoteRichPresentationAndEditing(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	source := "# 标题\n\n**加粗** *斜体* ~~删除~~\n> 引用\n- 项目\n```go\nfmt.Println(1)\n```"
	n.editor.SetText(source)
	n.editor.Resize(fyne.NewSize(500, 600))
	n.editor.Refresh()
	r := n.editor.richRenderer
	if r == nil {
		t.Fatal("rich renderer missing")
	}
	var visible strings.Builder
	heading, bold, italic, strike := false, false, false, false
	for _, o := range r.Objects() {
		if text, ok := o.(*canvas.Text); ok {
			visible.WriteString(text.Text)
			heading = heading || text.TextSize == 32
			bold = bold || text.TextStyle.Bold
			italic = italic || text.TextStyle.Italic
			strike = strike || text.TextStyle.Strikethrough
		}
	}
	for _, syntax := range []string{"# ", "**", "~~", "```", "> "} {
		if strings.Contains(visible.String(), syntax) {
			t.Fatalf("syntax visible: %q", visible.String())
		}
	}
	if !heading || !bold || !italic || !strike {
		t.Fatal("format styles missing")
	}
	if n.editor.Text != source {
		t.Fatal("render changed stored source")
	}
	n.editor.SetText("**中文**")
	n.editor.selectSource(2, 4)
	if n.editor.SelectedText() != "中文" {
		t.Fatalf("selection: %q", n.editor.SelectedText())
	}
	n.editor.TypedRune('字')
	if n.editor.Text != "**字**" {
		t.Fatalf("replacement: %q", n.editor.Text)
	}
	n.editor.Undo()
	if n.editor.Text != "**中文**" {
		t.Fatalf("undo: %q", n.editor.Text)
	}
	n.editor.selectSource(3, 3)
	n.editor.TypedRune('新')
	if n.editor.Text != "**中新文**" {
		t.Fatalf("caret insertion: %q", n.editor.Text)
	}
	n.editor.Refresh()
	p := r.nearestOffset(3)
	n.editor.MouseDown(&desktop.MouseEvent{PointEvent: fyne.PointEvent{Position: fyne.NewPos(p.position.X, p.position.Y+p.height/2)}, Button: desktop.MouseButtonPrimary})
	n.editor.TypedRune('点')
	if !strings.Contains(n.editor.Text, "点") {
		t.Fatal("mouse editing failed")
	}
	n.viewPicker.SetSelected("源码")
	if n.editor.rich {
		t.Fatal("source mode unavailable")
	}
	n.viewPicker.SetSelected("编辑")
	if !n.editor.rich {
		t.Fatal("rich mode not restored")
	}
}
func TestNoteRichWrapping(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText(strings.Repeat("自动换行 ", 30))
	n.editor.Resize(fyne.NewSize(240, 900))
	n.editor.Refresh()
	r := n.editor.richRenderer
	if r.height < 200 {
		t.Fatal("text did not wrap")
	}
	for _, object := range r.Objects() {
		if text, ok := object.(*canvas.Text); ok && text.Position().X+text.MinSize().Width > 250 {
			t.Fatalf("wrapped run exceeds width: %v", text.Size())
		}
	}
}

func TestNoteRichDeletePreservesFormatting(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("**中文**")
	n.editor.selectSource(6, 6)
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	if n.editor.Text != "**中**" {
		t.Fatalf("deleted delimiter: %q", n.editor.Text)
	}
	n.editor.Undo()
	if n.editor.Text != "**中文**" {
		t.Fatalf("undo: %q", n.editor.Text)
	}
	n.editor.selectSource(0, 0)
	n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyDelete})
	if n.editor.Text != "**文**" {
		t.Fatalf("forward delete: %q", n.editor.Text)
	}
}
