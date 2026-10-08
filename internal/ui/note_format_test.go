package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
)

func TestNoteHeadingFormatsParagraphFromToolbar(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("第一段\n这是选中的正文\n最后一段")
	// Reverse selection of part of a paragraph, followed by focus on the button.
	n.editor.selectSource(10, 6)
	toolbar := n.formatToolbar()
	var h1 *shellAlignedButton
	var find func(fyne.CanvasObject)
	find = func(o fyne.CanvasObject) {
		switch v := o.(type) {
		case *shellAlignedButton:
			if v.Text == "H1" {
				h1 = v
			}
		case *container.ThemeOverride:
			find(v.Content)
		case *fyne.Container:
			for _, child := range v.Objects {
				find(child)
			}
		}
	}
	find(toolbar)
	if h1 == nil {
		t.Fatal("H1 toolbar button missing")
	}
	w.Window.Canvas().Focus(h1)
	test.Tap(h1)
	want := "第一段\n# 这是选中的正文\n最后一段"
	if n.editor.Text != want {
		t.Fatalf("heading inserted inside selection: %q", n.editor.Text)
	}
	n.insertFormat("## ", "")
	want = "第一段\n## 这是选中的正文\n最后一段"
	if n.editor.Text != want {
		t.Fatalf("heading switch stacked syntax: %q", n.editor.Text)
	}
	n.insertFormat("## ", "")
	if n.editor.Text != want {
		t.Fatalf("repeated heading stacked syntax: %q", n.editor.Text)
	}
	var display strings.Builder
	n.editor.Refresh()
	heading := false
	for _, o := range test.WidgetRenderer(n.editor).Objects() {
		if text, ok := o.(*canvas.Text); ok {
			display.WriteString(text.Text)
			heading = heading || text.TextSize == 26
		}
	}
	if strings.Contains(display.String(), "#") || !heading {
		t.Fatalf("heading presentation: %q", display.String())
	}
}
func TestNoteHeadingAtCaretAndMultilineSelection(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("一段\n二段\n三段")
	n.editor.selectSource(1, 1)
	n.insertFormat("# ", "")
	if n.editor.Text != "# 一段\n二段\n三段" {
		t.Fatal(n.editor.Text)
	}
	n.editor.selectSource(0, 8)
	n.insertFormat("### ", "")
	if n.editor.Text != "### 一段\n### 二段\n三段" {
		t.Fatal(n.editor.Text)
	}
}
func TestNoteInlineToolbarTogglesWithoutStacking(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	for _, mark := range []string{"**", "*", "~~", "`"} {
		n.editor.SetText("中文正文")
		n.editor.selectSource(0, 4)
		n.insertFormat(mark, mark)
		if n.editor.Text != mark+"中文正文"+mark {
			t.Fatal(n.editor.Text)
		}
		n.insertFormat(mark, mark)
		if n.editor.Text != "中文正文" {
			t.Fatalf("toggle stacked %s: %q", mark, n.editor.Text)
		}
	}
	// Applying a heading preserves inline formatting rather than putting # in it.
	n.editor.SetText("**中文正文**")
	n.editor.selectSource(2, 6)
	n.insertFormat("# ", "")
	if n.editor.Text != "# **中文正文**" {
		t.Fatal(n.editor.Text)
	}
}
func TestNoteSetBlockRepairsStackedPrefixes(t *testing.T) {
	if got := noteSetBlock("## # 正文\n- 项目", "# "); got != "# 正文\n# 项目" {
		t.Fatal(got)
	}
}

func TestNoteCombinedInlineFormatsHideDelimiters(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("正文")
	n.editor.selectSource(0, 2)
	n.insertFormat("**", "**")
	n.insertFormat("*", "*")
	runs := noteInlineRuns(n.editor.Text, 0, 16, fyne.TextStyle{})
	if len(runs) != 1 || runs[0].text != "正文" || !runs[0].style.Bold || !runs[0].style.Italic {
		t.Fatalf("nested formatting: %#v", runs)
	}
}

func TestNoteHeadingReplacesCodeBlockType(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	n.editor.SetText("前文\n```\n代码正文\n```\n后文")
	n.editor.selectSource(8, 10)
	n.insertFormat("# ", "")
	if n.editor.Text != "前文\n# 代码正文\n后文" {
		t.Fatalf("heading stayed literal code: %q", n.editor.Text)
	}
}
