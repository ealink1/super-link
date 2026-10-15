package sqlworkbench

import (
	"strings"
	"unicode"
)

// CompletionQuery uses rune offsets, matching the native editor's cursor.
type CompletionQuery struct {
	Start, End    int
	Prefix        string
	Qualifier     string
	Quote         rune
	TablePosition bool
	Active        bool
	References    []CompletionReference
}

type CompletionReference struct{ Schema, Name, Alias string }

type completionToken struct {
	text       string
	start, end int
	quote      rune
	blocked    bool
	closed     bool
}

// AnalyzeCompletion avoids strings/comments and limits work on very large scripts.
func AnalyzeCompletion(source string, cursor int) CompletionQuery {
	q := CompletionQuery{Start: cursor, End: cursor}
	if len(source) > 256<<10 {
		return q
	}
	runes := []rune(source)
	if cursor < 0 || cursor > len(runes) {
		return q
	}
	tokens := completionTokens(runes)
	before := []completionToken{}
	statement := []completionToken{}
	for _, t := range tokens {
		if t.blocked {
			if cursor > t.start && (cursor < t.end || cursor == t.end && !t.closed) {
				return q
			}
			continue
		}
		if t.text == ";" {
			if t.end <= cursor {
				before, statement = nil, nil
				continue
			}
			break
		}
		statement = append(statement, t)
		if cursor > t.start && cursor <= t.end && (completionWord(runes[t.start]) || t.quote != 0) {
			q.Start, q.End, q.Quote = t.start, t.end, t.quote
			start, end := t.start, cursor
			if t.quote != 0 {
				start++
				if t.closed && end == t.end {
					end--
				}
			}
			q.Prefix = string(runes[start:end])
		} else if t.end <= cursor {
			before = append(before, t)
		}
	}
	if q.Start > 0 && strings.ContainsRune(":@$", runes[q.Start-1]) {
		return q
	}
	if q.Start > 1 && string(runes[q.Start-2:q.Start]) == "${" {
		return q
	}
	if len(before) > 0 && before[len(before)-1].text == "." {
		before = before[:len(before)-1]
		if len(before) > 0 {
			q.Qualifier = before[len(before)-1].text
			before = before[:len(before)-1]
		}
	}
	if len(before) > 0 {
		last := strings.ToUpper(before[len(before)-1].text)
		q.TablePosition = strings.Contains(" FROM JOIN UPDATE INTO TABLE ", " "+last+" ")
	}
	q.References = completionReferences(statement)
	q.Active = q.End-q.Start <= 512
	return q
}

func completionWord(c rune) bool {
	return unicode.IsLetter(c) || unicode.IsDigit(c) || c == '_' || c == '$'
}

func completionTokens(source []rune) []completionToken {
	tokens := make([]completionToken, 0, min(len(source), 256))
	for i := 0; i < len(source) && len(tokens) < 20000; {
		if unicode.IsSpace(source[i]) {
			i++
			continue
		}
		t := completionToken{start: i, closed: true}
		c := source[i]
		i++
		switch {
		case c == '-' && i < len(source) && source[i] == '-':
			t.blocked = true
			for i < len(source) && source[i] != '\n' {
				i++
			}
			t.closed = i < len(source)
		case c == '/' && i < len(source) && source[i] == '*':
			t.blocked = true
			i++
			depth := 1
			for i < len(source) && depth > 0 {
				if i+1 < len(source) && source[i] == '/' && source[i+1] == '*' {
					depth++
					i += 2
				} else if i+1 < len(source) && source[i] == '*' && source[i+1] == '/' {
					depth--
					i += 2
				} else {
					i++
				}
			}
			t.closed = depth == 0
		case c == '\'' || c == '"' || c == '`' || c == '[':
			t.blocked, t.closed = c == '\'', false
			t.quote = c
			end := c
			if c == '[' {
				end = ']'
			}
			for i < len(source) {
				if source[i] == '\\' && c == '\'' && i+1 < len(source) {
					i += 2
				} else if source[i] == end {
					i++
					if i < len(source) && source[i] == end {
						i++
						continue
					}
					t.closed = true
					break
				} else {
					i++
				}
			}
		case c == '$':
			// PostgreSQL dollar strings may contain arbitrary SQL and semicolons.
			j := i
			for j < len(source) && (unicode.IsLetter(source[j]) || unicode.IsDigit(source[j]) || source[j] == '_') {
				j++
			}
			if j < len(source) && source[j] == '$' {
				delimiter := string(source[t.start : j+1])
				t.blocked, t.closed = true, false
				i = j + 1
				for i < len(source) {
					if strings.HasPrefix(string(source[i:min(len(source), i+len([]rune(delimiter)))]), delimiter) {
						i += len([]rune(delimiter))
						t.closed = true
						break
					}
					i++
				}
			}
		case completionWord(c):
			for i < len(source) && completionWord(source[i]) {
				i++
			}
		}
		t.end, t.text = i, string(source[t.start:i])
		if t.quote != 0 && !t.blocked {
			end := i
			if t.closed {
				end--
			}
			t.text = string(source[t.start+1 : end])
		}
		tokens = append(tokens, t)
	}
	return tokens
}

func completionReferences(tokens []completionToken) []CompletionReference {
	refs := []CompletionReference{}
	for i := 0; i+1 < len(tokens) && len(refs) < 16; i++ {
		if !strings.Contains(" FROM JOIN UPDATE INTO ", " "+strings.ToUpper(tokens[i].text)+" ") || tokens[i].quote != 0 {
			continue
		}
		i++
		if tokens[i].text == "(" {
			continue
		}
		r := CompletionReference{Name: tokens[i].text}
		if i+2 < len(tokens) && tokens[i+1].text == "." {
			r.Schema, r.Name = r.Name, tokens[i+2].text
			i += 2
		}
		if i+1 < len(tokens) && strings.EqualFold(tokens[i+1].text, "AS") {
			i++
		}
		if i+1 < len(tokens) && len(tokens[i+1].text) > 0 && (tokens[i+1].quote != 0 || !keywords[strings.ToUpper(tokens[i+1].text)] && completionWord([]rune(tokens[i+1].text)[0])) {
			r.Alias = tokens[i+1].text
		}
		refs = append(refs, r)
	}
	return refs
}
