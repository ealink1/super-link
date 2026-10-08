package ui

import (
	"fmt"
	"strconv"
	"strings"
)

// Lists and quotes continue when entering another paragraph. Enter on an empty
// item exits that paragraph type instead of leaving a stranded Markdown mark.
func (e *noteEntry) continueNoteParagraph() bool {
	if e.SelectedText() != "" {
		return false
	}
	source := []rune(e.Text)
	cursor := e.sourceOffset()
	start, end := noteParagraphRange(source, cursor, cursor)
	if _, _, _, code := noteEnclosingCodeBlock(e.Text, start, end); code {
		return false
	}
	line := string(source[start:end])
	prefix := ""
	switch {
	case strings.HasPrefix(line, "- "), strings.HasPrefix(line, "* "), strings.HasPrefix(line, "+ "):
		prefix = line[:2]
	case strings.HasPrefix(line, "> "):
		prefix = "> "
	default:
		if marker := noteOrdered.FindString(line); marker != "" {
			number, err := strconv.Atoi(strings.TrimSuffix(strings.TrimSpace(marker), "."))
			if err != nil {
				return false
			}
			prefix = fmt.Sprintf("%d. ", number+1)
		}
	}
	if prefix == "" {
		return false
	}
	if strings.TrimSpace(noteBlockPrefix.ReplaceAllString(line, "")) == "" {
		e.replaceFormatRange(start, end, "", 0, 0)
		return true
	}
	value := "\n" + prefix
	length := len([]rune(value))
	e.replaceFormatRange(cursor, cursor, value, length, length)
	return true
}
