package ui

import (
	"strings"
	"unicode"

	"fyne.io/fyne/v2"
)

type noteStyledPart struct {
	text     string
	style    fyne.TextStyle
	selected bool
}

// Markdown emphasis cannot cross blank paragraphs. Rebuild each selected
// paragraph independently, merging styles instead of stacking delimiters.
func noteMultilineFormat(source string, start, end int, mark string) (string, int, int) {
	lines := strings.Split(source, "\n")
	active := noteRangeHasStyle(source, start, end, mark)
	offset, outputOffset := 0, 0
	selectionStart, selectionEnd := -1, -1
	for index, raw := range lines {
		length := len([]rune(raw))
		if offset < end && offset+length > start {
			prefix := noteBlockPrefix.FindString(raw)
			body := strings.TrimPrefix(raw, prefix)
			runs := noteInlineRuns(body, offset+len([]rune(prefix)), 16, fyne.TextStyle{})
			// Preserve unsupported Markdown syntax, including link destinations.
			// Only the inline styles understood by the editor are canonicalized.
			if noteInlineSourceSupported(body, runs) {
				parts := noteStyledParts(runs, start, end, mark, active)
				var rendered strings.Builder
				renderedLength := len([]rune(prefix))
				rendered.WriteString(prefix)
				for _, part := range parts {
					leading := part.text[:len(part.text)-len(strings.TrimLeftFunc(part.text, unicode.IsSpace))]
					rest := part.text[len(leading):]
					core := strings.TrimRightFunc(rest, unicode.IsSpace)
					trailing := rest[len(core):]
					open, close := noteInlineMarkers(part.style)
					if core == "" {
						open, close = "", ""
					}
					rendered.WriteString(leading)
					rendered.WriteString(open)
					renderedLength += len([]rune(leading + open))
					position := outputOffset + renderedLength
					if part.selected && selectionStart < 0 {
						selectionStart = position - len([]rune(leading))
					}
					rendered.WriteString(core)
					renderedLength += len([]rune(core))
					if part.selected {
						selectionEnd = outputOffset + renderedLength + len([]rune(trailing))
					}
					rendered.WriteString(close)
					rendered.WriteString(trailing)
					renderedLength += len([]rune(close + trailing))
				}
				lines[index] = rendered.String()
			} else {
				// Keep metadata intact; delimiters still stay within one line.
				localStart, localEnd := max(start-offset, len([]rune(prefix))), min(end-offset, length)
				chars := []rune(raw)
				value := string(chars[localStart:localEnd])
				// Whole-line selections retain link/image destinations while
				// toggling the canonical outer style applied by this toolbar.
				first, last := -1, -1
				var existing fyne.TextStyle
				uniform := true
				for _, run := range runs {
					if strings.TrimSpace(run.text) == "" {
						continue
					}
					if first < 0 {
						first = run.start - offset
						existing = run.style
					} else if existing != run.style {
						uniform = false
					}
					last = run.start - offset + len([]rune(run.text))
				}
				if uniform && first >= 0 && localStart <= first && localEnd >= last {
					leading := body[:len(body)-len(strings.TrimLeftFunc(body, unicode.IsSpace))]
					core := strings.TrimSpace(body)
					trailing := body[len(strings.TrimRightFunc(body, unicode.IsSpace)):]
					open, close := noteInlineMarkers(existing)
					if strings.HasPrefix(core, open) && strings.HasSuffix(core, close) && len(core) >= len(open)+len(close) {
						core = core[len(open) : len(core)-len(close)]
						noteSetInlineStyle(&existing, mark, !active)
						open, close = noteInlineMarkers(existing)
						lines[index] = prefix + leading + open + core + close + trailing
						if selectionStart < 0 {
							selectionStart = outputOffset + len([]rune(prefix+leading+open))
						}
						selectionEnd = outputOffset + len([]rune(prefix+leading+open+core))
					} else {
						lines[index] = string(chars[:localStart]) + mark + value + mark + string(chars[localEnd:])
					}
				} else {
					lines[index] = string(chars[:localStart]) + mark + value + mark + string(chars[localEnd:])
				}
				if selectionStart < 0 {
					selectionStart = outputOffset + localStart
				}
				if selectionEnd < outputOffset {
					selectionEnd = outputOffset + len([]rune(lines[index]))
				}

			}
		}
		offset += length + 1
		outputOffset += len([]rune(lines[index])) + 1
	}
	if selectionStart < 0 {
		selectionStart, selectionEnd = start, end
	}
	return strings.Join(lines, "\n"), selectionStart, selectionEnd
}

func noteInlineSourceSupported(source string, runs []noteRun) bool {
	var plain strings.Builder
	for _, run := range runs {
		plain.WriteString(run.text)
	}
	strip := func(value string) string {
		return strings.Map(func(ch rune) rune {
			if ch == '*' || ch == '~' || ch == '`' {
				return -1
			}
			return ch
		}, value)
	}
	return strip(source) == strip(plain.String())
}
func noteSetInlineStyle(style *fyne.TextStyle, mark string, enabled bool) {
	switch mark {
	case "**":
		style.Bold = enabled
	case "*":
		style.Italic = enabled
	case "~~":
		style.Strikethrough = enabled
	case "`":
		style.Monospace = enabled
	}
}
func noteInlineMarkers(style fyne.TextStyle) (string, string) {
	outer, inner := "", ""
	if style.Strikethrough {
		outer = "~~"
	}
	if style.Bold {
		inner += "**"
	}
	if style.Italic {
		inner += "*"
	}
	code := ""
	if style.Monospace {
		code = "`"
	}
	return outer + inner + code, code + inner + outer
}

func noteStyledParts(runs []noteRun, start, end int, mark string, active bool) []noteStyledPart {
	var parts []noteStyledPart
	var partText strings.Builder
	for _, run := range runs {
		for i, ch := range []rune(run.text) {
			selected := run.start+i >= start && run.start+i < end
			style := run.style
			if selected {
				noteSetInlineStyle(&style, mark, !active)
			}
			if len(parts) > 0 && parts[len(parts)-1].style == style && parts[len(parts)-1].selected == selected {
				partText.WriteRune(ch)
			} else {
				if len(parts) > 0 {
					parts[len(parts)-1].text = partText.String()
					partText.Reset()
				}
				parts = append(parts, noteStyledPart{"", style, selected})
				partText.WriteRune(ch)
			}
		}
	}
	if len(parts) > 0 {
		parts[len(parts)-1].text = partText.String()
	}

	return parts
}
