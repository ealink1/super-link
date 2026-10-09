package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
)

func TestNoteBulkInlineStylesAndToggle(t *testing.T) {
	w, n := noteTestWindow(t)
	w.Window.SetContent(n.content)
	n.newNote()
	plain := "服务地址：example.invalid\n\n端口：21\n模式：被动模式\n\n下载说明"
	n.editor.SetText(plain)
	n.editor.selectSource(0, len([]rune(plain)))
	for _, mark := range []string{"**", "*", "~~"} {
		n.insertFormat(mark, mark)
	}
	assertBulkDisplay(t, n.editor.Text, plain, true, true, true)
	n.insertFormat("*", "*")
	assertBulkDisplay(t, n.editor.Text, plain, true, false, true)
	n.insertFormat("~~", "~~")
	assertBulkDisplay(t, n.editor.Text, plain, true, false, false)
	n.insertFormat("**", "**")
	if n.editor.Text != plain {
		t.Fatalf("toggle leaves syntax: %q", n.editor.Text)
	}
	n.editor.TypedShortcut(&fyne.ShortcutUndo{})
	assertBulkDisplay(t, n.editor.Text, plain, true, false, false)
}

func assertBulkDisplay(t *testing.T, source, plain string, bold, italic, strike bool) {
	t.Helper()
	var displayed []string
	for _, line := range notePresentation(source) {
		var text strings.Builder
		for _, run := range line.runs {
			text.WriteString(run.text)
			if strings.TrimSpace(run.text) != "" && (run.style.Bold != bold || run.style.Italic != italic || run.style.Strikethrough != strike) {
				t.Fatalf("wrong style in %q: %#v", source, run)
			}
		}
		displayed = append(displayed, text.String())
	}
	if strings.Join(displayed, "\n") != plain {
		t.Fatalf("visible delimiters or lost content: %q", strings.Join(displayed, "\n"))
	}
}

func TestNoteBulkStylesKeepBlocksSpacesAndMetadata(t *testing.T) {
	source := "# 标题\n\n- 项目\n  正文  \n[链接](https://example.invalid/path)"
	result, _, _ := noteMultilineFormat(source, 0, len([]rune(source)), "**")
	if !strings.HasPrefix(result, "# **标题**\n\n- **项目**\n  **正文**  \n") || !strings.Contains(result, "https://example.invalid/path") {
		t.Fatal(result)
	}
}

func TestNoteBulkPartialSelectionKeepsExistingStyle(t *testing.T) {
	source := "**前后正文**\n**第二段正文**"
	result, _, _ := noteMultilineFormat(source, 4, 14, "*")
	lines := notePresentation(result)
	if len(lines) != 2 {
		t.Fatal(result)
	}
	for _, line := range lines {
		for _, run := range line.runs {
			if strings.ContainsAny(run.text, "*~") || !run.style.Bold {
				t.Fatalf("broken partial style: %q %#v", result, run)
			}
		}
	}
}

func TestNoteBulkLinkStylesPreserveDestinationWhenToggled(t *testing.T) {
	source := "正文\n[链接](https://example.invalid/path)"
	result, a, b := noteMultilineFormat(source, 0, len([]rune(source)), "**")
	result, a, b = noteMultilineFormat(result, a, b, "*")
	result, a, b = noteMultilineFormat(result, a, b, "*")
	result, _, _ = noteMultilineFormat(result, a, b, "**")
	if result != source {
		t.Fatalf("link changed after toggle: %q", result)
	}
}
