package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestEveryNoteFormatButton(t *testing.T) {
	cases := []struct {
		prefix, suffix, want string
		toggle               bool
	}{
		{"**", "**", "**正文**", true}, {"*", "*", "*正文*", true},
		{"~~", "~~", "~~正文~~", true}, {"`", "`", "`正文`", true},
		{"# ", "", "# 正文", false}, {"## ", "", "## 正文", false},
		{"### ", "", "### 正文", false}, {"#### ", "", "#### 正文", false},
		{"- ", "", "- 正文", true}, {"1. ", "", "1. 正文", true},
		{"> ", "", "> 正文", true},
	}
	for _, tc := range cases {
		t.Run(tc.prefix, func(t *testing.T) {
			w, n := noteTestWindow(t)
			w.Window.SetContent(n.content)
			n.newNote()
			n.editor.SetText("正文")
			n.editor.selectSource(2, 0)
			var button *shellAlignedButton
			for _, item := range n.formatButtons {
				if item.prefix == tc.prefix && item.suffix == tc.suffix {
					button = item.button
				}
			}
			if button == nil {
				t.Fatal("toolbar action missing")
			}
			w.Window.Canvas().Focus(button)
			test.Tap(button)
			if n.editor.Text != tc.want {
				t.Fatalf("apply: %q", n.editor.Text)
			}
			if button.Importance != widget.HighImportance {
				t.Fatal("active format not highlighted")
			}
			n.editor.Refresh()
			var display strings.Builder
			for _, o := range test.WidgetRenderer(n.editor).Objects() {
				if text, ok := o.(*canvas.Text); ok {
					display.WriteString(text.Text)
				}
			}
			for _, syntax := range []string{"**", "~~", "```", "# ", "> ", "- "} {
				if strings.Contains(display.String(), syntax) {
					t.Fatalf("syntax shown: %q", display.String())
				}
			}
			n.editor.TypedShortcut(&fyne.ShortcutUndo{})
			if n.editor.Text != "正文" {
				t.Fatalf("one-step undo: %q", n.editor.Text)
			}
			n.editor.TypedShortcut(&fyne.ShortcutRedo{})
			if n.editor.Text != tc.want {
				t.Fatalf("redo: %q", n.editor.Text)
			}
			// Re-select via a caret inside the rendered content after redo.
			n.editor.selectSource(len([]rune(tc.prefix)), len([]rune(tc.want))-len([]rune(tc.suffix)))
			test.Tap(button)
			expected := tc.want
			if tc.toggle {
				expected = "正文"
			}
			if n.editor.Text != expected {
				t.Fatalf("repeat/toggle: %q", n.editor.Text)
			}
		})
	}
}
func TestNoteParagraphFormatTransitions(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("第一项\n第二项\n第三项")
	n.editor.selectSource(0, len([]rune(n.editor.Text)))
	n.insertFormat("1. ", "")
	if n.editor.Text != "1. 第一项\n2. 第二项\n3. 第三项" {
		t.Fatal(n.editor.Text)
	}
	n.insertFormat("- ", "")
	if n.editor.Text != "- 第一项\n- 第二项\n- 第三项" {
		t.Fatal(n.editor.Text)
	}
	n.insertFormat("> ", "")
	if n.editor.Text != "> 第一项\n> 第二项\n> 第三项" {
		t.Fatal(n.editor.Text)
	}

}
func TestNotePartialInlineTogglePreservesSurroundingText(t *testing.T) {
	for _, mark := range []string{"**", "*", "~~", "`"} {
		t.Run(mark, func(t *testing.T) {
			w, n := noteTestWindow(t)
			w.Window.SetContent(n.content)
			n.newNote()
			n.editor.SetText(mark + "甲乙丙丁" + mark)
			length := len([]rune(mark))
			n.editor.selectSource(length+1, length+3)
			n.insertFormat(mark, mark)
			if noteRangeHasStyle(n.editor.Text, length*2+1, length*2+3, mark) {
				t.Fatalf("selected format still active: %q", n.editor.Text)
			}
			var visible strings.Builder
			for _, run := range noteInlineRuns(n.editor.Text, 0, 16, fyne.TextStyle{}) {
				visible.WriteString(run.text)
			}
			if visible.String() != "甲乙丙丁" {
				t.Fatalf("toggle lost text: %q", visible.String())
			}
		})
	}
}

func TestNoteListAndQuoteContinueAndExit(t *testing.T) {
	for _, item := range []struct{ source, want string }{{"- 条目", "- 条目\n- "}, {"2. 条目", "2. 条目\n3. "}, {"> 引用", "> 引用\n> "}} {
		w, n := noteTestWindow(t)
		w.Window.SetContent(n.content)
		n.newNote()
		n.editor.SetText(item.source)
		cursor := len([]rune(item.source))
		n.editor.selectSource(cursor, cursor)
		n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
		if n.editor.Text != item.want {
			t.Fatalf("continue: %q", n.editor.Text)
		}
		n.editor.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
		if n.editor.Text != item.source+"\n" {
			t.Fatalf("exit: %q", n.editor.Text)
		}
		n.editor.Undo()
		if n.editor.Text != item.want {
			t.Fatalf("undo exit: %q", n.editor.Text)
		}
	}
}

func TestNoteCodeBlockCreationRemoved(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	for _, item := range n.formatButtons {
		if item.button.Text == "代码块" || strings.Contains(item.prefix, "```") {
			t.Fatal("code block action still available")
		}
	}
	n.editor.SetText("正文")
	n.editor.selectSource(0, 2)
	n.insertFormat("\n```\n", "\n```\n")
	if n.editor.Text != "正文" {
		t.Fatal("removed action modified note")
	}
}
