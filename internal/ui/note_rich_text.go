package ui

import (
	"regexp"
	"strings"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"github.com/yuin/goldmark"
	"github.com/yuin/goldmark/ast"
	"github.com/yuin/goldmark/extension"
	extensionast "github.com/yuin/goldmark/extension/ast"
	goldtext "github.com/yuin/goldmark/text"
)

// A presentation run retains source offsets so editing continues to operate on
// the original Markdown, including syntax we do not yet render specially.
type noteRun struct {
	text  string
	start int
	size  float32
	style fyne.TextStyle
}
type noteLine struct {
	runs        []noteRun
	start, end  int
	size        float32
	marker      string
	quote, code bool
}

var noteOrdered = regexp.MustCompile(`^\d+\. `)

func notePresentation(source string) []noteLine {
	lines := strings.Split(source, "\n")
	result := make([]noteLine, 0, len(lines))
	offset, fenced := 0, false
	for _, raw := range lines {
		line := noteLine{start: offset, end: offset + utf8.RuneCountInString(raw), size: 16}
		text, prefix := raw, ""
		style := fyne.TextStyle{}
		if strings.HasPrefix(strings.TrimSpace(raw), "```") {
			fenced = !fenced
			line.code = true
			result = append(result, line)
			offset = line.end + 1
			continue
		}
		line.code = fenced
		if fenced {
			style.Monospace = true
		} else {
			for level := 4; level >= 1; level-- {
				candidate := strings.Repeat("#", level) + " "
				if strings.HasPrefix(text, candidate) {
					prefix = candidate
					line.size = []float32{32, 26, 22, 18}[level-1]
					style.Bold = true
					break
				}
			}
			if prefix == "" {
				switch {
				case strings.HasPrefix(text, "> "):
					prefix = "> "
					line.quote = true
				case strings.HasPrefix(text, "- "), strings.HasPrefix(text, "* "):
					prefix = text[:2]
					line.marker = "•"
				default:
					if p := noteOrdered.FindString(text); p != "" {
						prefix = p
						line.marker = strings.TrimSpace(p)
					}
				}
			}
		}
		text = strings.TrimPrefix(text, prefix)
		start := offset + utf8.RuneCountInString(prefix)
		if fenced {
			line.runs = []noteRun{{text, start, line.size, style}}
		} else {
			line.runs = noteInlineRuns(text, start, line.size, style)
		}
		result = append(result, line)
		offset = line.end + 1
	}
	return result
}

// Use the Markdown parser for nested emphasis; delimiter searches alone break
// combinations such as bold plus italic (***text***).
var noteInlineMarkdown = goldmark.New(goldmark.WithExtensions(extension.Strikethrough))

func noteInlineRuns(value string, start int, size float32, style fyne.TextStyle) []noteRun {
	source := []byte(value)
	document := noteInlineMarkdown.Parser().Parse(goldtext.NewReader(source))
	var runs []noteRun
	_ = ast.Walk(document, func(node ast.Node, entering bool) (ast.WalkStatus, error) {
		if !entering {
			return ast.WalkContinue, nil
		}
		text, ok := node.(*ast.Text)
		if !ok {
			return ast.WalkContinue, nil
		}
		nested := style
		for parent := node.Parent(); parent != nil; parent = parent.Parent() {
			switch v := parent.(type) {
			case *ast.Emphasis:
				if v.Level == 2 {
					nested.Bold = true
				} else {
					nested.Italic = true
				}
			case *ast.CodeSpan:
				nested.Monospace = true
			case *extensionast.Strikethrough:
				nested.Strikethrough = true
			}
		}
		content := string(text.Segment.Value(source))
		runs = append(runs, noteRun{content, start + utf8.RuneCount(source[:text.Segment.Start]), size, nested})
		return ast.WalkContinue, nil
	})
	// Keep unrecognised content editable, including standalone punctuation.
	if len(runs) == 0 && value != "" {
		runs = []noteRun{{value, start, size, style}}
	}
	return runs
}
