package ui

import (
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

func (n *noteWorkspace) formatToolbar() *fyne.Container {
	objects := []fyne.CanvasObject{}
	n.formatButtons = nil
	for _, format := range []struct{ name, icon, prefix, suffix string }{
		{"", "bold", "**", "**"}, {"", "italic", "*", "*"}, {"", "strikethrough", "~~", "~~"}, {"", "code", "`", "`"},
		{"H1", "", "# ", ""}, {"H2", "", "## ", ""}, {"H3", "", "### ", ""}, {"H4", "", "#### ", ""},
		{"", "list", "- ", ""}, {"", "list-ordered", "1. ", ""}, {"", "quote", "> ", ""},
	} {
		item := format
		button := shellButton(item.name, item.icon, false, func() { n.insertFormat(item.prefix, item.suffix) })
		if item.name != "" {
			button.textOffset = 2
		}
		n.formatButtons = append(n.formatButtons, noteFormatButton{item.prefix, item.suffix, button})
		objects = append(objects, noteButtonView(button), shellFixed(layout.NewSpacer(), 4, 0))
	}
	objects = append(objects, noteButtonView(shellButton("", "undo-2", false, n.editor.Undo)), noteButtonView(shellButton("", "redo-2", false, n.editor.Redo)))
	return shellHBox(objects...)
}

type noteClipboard struct{ text string }

func (c *noteClipboard) Content() string         { return c.text }
func (c *noteClipboard) SetContent(value string) { c.text = value }
func (n *noteWorkspace) insertFormat(prefix, suffix string) {
	if strings.Contains(prefix, "```") {
		return
	}
	note := n.current()
	if note == nil || note.Deleted {
		return
	}
	n.applyNoteFormat(prefix, suffix)
	n.owner.Window.Canvas().Focus(n.editor)
	n.refreshFormatState()
}
func formatNoteSelection(value, prefix, suffix string) string {
	if suffix == "" {
		return prefix + strings.ReplaceAll(value, "\n", "\n"+prefix)
	}
	return prefix + value + suffix
}

func (n *noteWorkspace) renderPreview() {
	if n.preview == nil || n.editor == nil || n.view == "编辑" {
		return
	}
	candidate := widget.NewRichTextFromMarkdown(n.editor.Text)
	n.preview.Segments = safeNoteSegments(candidate.Segments)
	n.preview.Refresh()
}

// Images stay as text; displaying a local note never fetches remote images or
// reads arbitrary filesystem paths. Explicit HTTP(S) links remain clickable.
func safeNoteSegments(segments []widget.RichTextSegment) []widget.RichTextSegment {
	for i, s := range segments {
		switch v := s.(type) {
		case *widget.ImageSegment:
			segments[i] = &widget.TextSegment{Text: v.Textual(), Style: widget.RichTextStyleInline}
		case *widget.HyperlinkSegment:
			if v.URL == nil || v.URL.Scheme != "https" && v.URL.Scheme != "http" && v.URL.Scheme != "mailto" {
				segments[i] = &widget.TextSegment{Text: v.Text, Style: widget.RichTextStyleInline}
			}
		case *widget.ParagraphSegment:
			v.Texts = safeNoteSegments(v.Texts)
		case *widget.ListSegment:
			v.Items = safeNoteSegments(v.Items)
		case *widget.TableSegment:
			for j, cell := range v.Headers {
				v.Headers[j] = safeNoteSegments(cell)
			}
			for j, row := range v.Rows {
				for k, cell := range row {
					v.Rows[j][k] = safeNoteSegments(cell)
				}
			}
		}
	}
	return segments
}
