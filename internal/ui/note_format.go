package ui

import (
	"fmt"
	"regexp"
	"strings"

	"fyne.io/fyne/v2"
)

var noteBlockPrefix = regexp.MustCompile(`^(#{1,6}[ \t]+|> |[-*+] |[0-9]+\. )`)

func (e *noteEntry) sourceSelection() (int, int) {
	end := e.sourceOffset()
	length := len([]rune(e.SelectedText()))
	if length == 0 {
		return end, end
	}
	if e.selectionAnchor > end {
		return end, end + length
	}
	return end - length, end
}

// Headings and lists belong to entire paragraphs, even when only a word in
// that paragraph is selected. Changing their type replaces its existing mark.
func noteParagraphRange(text []rune, start, end int) (int, int) {
	start = max(0, min(start, len(text)))
	end = max(start, min(end, len(text)))
	for start > 0 && text[start-1] != '\n' {
		start--
	}
	// A selection ending at the start of the next paragraph excludes that line.
	if end > start && end > 0 && text[end-1] == '\n' {
		end--
	}
	for end < len(text) && text[end] != '\n' {
		end++
	}
	return start, end
}
func noteSetBlock(value, prefix string) string {
	lines := strings.Split(value, "\n")
	for i, line := range lines {
		// Also repair stacked paragraph markers produced by the former toolbar.
		for {
			previous := line
			line = noteBlockPrefix.ReplaceAllString(line, "")
			if line == previous {
				break
			}
		}
		marker := prefix
		if prefix == "1. " {
			marker = fmt.Sprintf("%d. ", i+1)
		}
		lines[i] = marker + line
	}
	return strings.Join(lines, "\n")
}
func (n *noteWorkspace) applyNoteFormat(prefix, suffix string) {
	e := n.editor
	source := []rune(e.Text)
	start, end := e.sourceSelection()
	start = max(0, min(start, len(source)))
	end = max(start, min(end, len(source)))
	if suffix == "" {
		start, end = noteParagraphRange(source, start, end)
		value := string(source[start:end])
		if blockStart, blockEnd, body, ok := noteEnclosingCodeBlock(e.Text, start, end); ok {
			start, end, value = blockStart, blockEnd, body
		}
		applied := prefix
		if !strings.HasPrefix(prefix, "#") && noteBlocksActive(value, prefix) {
			applied = ""
		}
		replacement := noteSetBlock(value, applied)
		e.replaceFormatRange(start, end, replacement, len([]rune(applied)), len([]rune(replacement)))
		return
	}
	// With a caret, format the current word; an empty paragraph remains editable.
	if start == end && len(source) > 0 {
		for start > 0 && !noteWordBoundary(source[start-1]) {
			start--
		}
		for end < len(source) && !noteWordBoundary(source[end]) {
			end++
		}
	}
	if strings.Contains(string(source[start:end]), "\n") {
		replacement, selectedStart, selectedEnd := noteMultilineFormat(e.Text, start, end, prefix)
		e.replaceFormatRange(0, len(source), replacement, selectedStart, selectedEnd)
		return
	}
	value := string(source[start:end])
	active := noteRangeHasStyle(e.Text, start, end, prefix)
	mark := []rune(prefix)
	tail := []rune(suffix)
	// The rendered selection usually excludes its invisible delimiters.
	if active && start >= len(mark) && end+len(tail) <= len(source) && string(source[start-len(mark):start]) == prefix && string(source[end:end+len(tail)]) == suffix {
		e.replaceFormatRange(start-len(mark), end+len(tail), value, 0, len([]rune(value)))
		return
	}
	// Select-all or source-mode selections can include those delimiters.
	if active && strings.HasPrefix(value, prefix) && strings.HasSuffix(value, suffix) && len([]rune(value)) >= len(mark)+len(tail) {
		plain := string([]rune(value)[len(mark) : len([]rune(value))-len(tail)])
		e.replaceFormatRange(start, end, plain, 0, len([]rune(plain)))
		return
	}
	if value == "" {
		value = "内容"
	}
	replacement := prefix + value + suffix
	e.replaceFormatRange(start, end, replacement, len(mark), len(mark)+len([]rune(value)))
}
func noteWordBoundary(ch rune) bool { return ch == '\n' || ch == ' ' || ch == '\t' }
func (e *noteEntry) replaceFormatRange(start, end int, replacement string, selectStart, selectEnd int) {
	e.selectSource(start, end)
	if replacement == "" {
		e.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyBackspace})
	} else {
		e.Entry.TypedShortcut(&fyne.ShortcutPaste{Clipboard: &noteClipboard{text: replacement}})
	}
	e.selectSource(start+selectStart, start+selectEnd)
	e.Refresh()
}

func noteRangeHasStyle(source string, start, end int, mark string) bool {
	found := false
	for _, line := range notePresentation(source) {
		for _, run := range line.runs {
			if strings.TrimSpace(run.text) == "" {
				continue
			}
			if run.start >= end || run.start+len([]rune(run.text)) <= start {
				continue
			}
			active := false
			switch mark {
			case "**":
				active = run.style.Bold
			case "*":
				active = run.style.Italic
			case "~~":
				active = run.style.Strikethrough
			case "`":
				active = run.style.Monospace
			}
			if !active {
				return false
			}
			found = true
		}
	}
	return found
}

// A code block is one paragraph type. Changing it to a heading/list removes
// its fences so the requested style is not displayed as literal code.
func noteEnclosingCodeBlock(source string, start, end int) (int, int, string, bool) {
	lines := strings.Split(source, "\n")
	open, contentStart, offset := -1, 0, 0
	for _, line := range lines {
		next := offset + len([]rune(line))
		if strings.HasPrefix(strings.TrimSpace(line), "```") {
			if open < 0 {
				open = offset
				contentStart = next + 1
			} else {
				if start >= contentStart && end <= offset {
					runes := []rune(source)
					bodyEnd := max(contentStart, offset-1)
					return open, next, string(runes[contentStart:bodyEnd]), true
				}
				open = -1
			}
		}
		offset = next + 1
	}
	return 0, 0, "", false
}

func noteBlocksActive(value, prefix string) bool {
	found := false
	for _, line := range strings.Split(value, "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		active := strings.HasPrefix(line, prefix)
		if prefix == "1. " {
			active = noteOrdered.MatchString(line)
		}
		if prefix == "- " {
			active = strings.HasPrefix(line, "- ") || strings.HasPrefix(line, "* ") || strings.HasPrefix(line, "+ ")
		}
		if !active {
			return false
		}
		found = true
	}
	return found
}
