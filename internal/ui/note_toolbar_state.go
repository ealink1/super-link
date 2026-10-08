package ui

import (
	"strings"

	"fyne.io/fyne/v2/widget"
)

type noteFormatButton struct {
	prefix, suffix string
	button         *shellAlignedButton
}

func (n *noteWorkspace) refreshFormatState() {
	if n.editor == nil || len(n.formatButtons) == 0 {
		return
	}
	e := n.editor
	start, end := e.sourceSelection()
	source := []rune(e.Text)
	start = max(0, min(start, len(source)))
	end = max(start, min(end, len(source)))
	paragraphStart, paragraphEnd := noteParagraphRange(source, start, end)
	paragraph := string(source[paragraphStart:paragraphEnd])
	_, _, _, code := noteEnclosingCodeBlock(e.Text, paragraphStart, paragraphEnd)
	if start == end {
		if end < len(source) {
			end++
		} else if start > 0 {
			start--
		}
	}
	for _, item := range n.formatButtons {
		active := false
		disabled := n.current() == nil || n.current().Deleted || (code && item.suffix != "" && !strings.Contains(item.prefix, "```"))
		if disabled && !item.button.Disabled() {
			item.button.Disable()
		} else if !disabled && item.button.Disabled() {
			item.button.Enable()
		}
		switch {
		case strings.Contains(item.prefix, "```"):
			active = code
		case item.suffix == "":
			active = !code && noteBlocksActive(paragraph, item.prefix)
		default:
			active = !code && noteRangeHasStyle(e.Text, start, end, item.prefix)
		}
		importance := widget.LowImportance
		if active {
			importance = widget.HighImportance
		}
		if item.button.Importance != importance {
			item.button.Importance = importance
			item.button.Refresh()
		}
	}
}
