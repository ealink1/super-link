package sqlworkbench

import (
	"sort"
	"strings"
	"unicode"
)

type CompletionTable struct {
	Schema, Name string
	Columns      []string
}

type CompletionCandidate struct{ Text, Insert, Kind, Detail string }

// Complete only reads a bounded metadata snapshot. It performs no database I/O.
func Complete(q CompletionQuery, tables []CompletionTable, dialect string) []CompletionCandidate {
	if !q.Active {
		return nil
	}
	result := []CompletionCandidate{}
	seen := map[string]bool{}
	add := func(name, kind, detail string, identifier bool) {
		if len(result) >= 60 || len(name) > 512 || !strings.HasPrefix(strings.ToLower(name), strings.ToLower(q.Prefix)) {
			return
		}
		insert := name
		if identifier {
			insert = completionIdentifier(name, dialect, q.Quote)
		} else if q.Prefix != "" && q.Prefix == strings.ToLower(q.Prefix) {
			insert = strings.ToLower(name)
		}
		if insert == "" || seen[insert] {
			return
		}
		if q.Quote == 0 && insert == q.Prefix && q.End-q.Start == len([]rune(q.Prefix)) {
			return
		}
		seen[insert] = true
		result = append(result, CompletionCandidate{Text: name, Insert: insert, Kind: kind, Detail: detail})
	}
	ordered := append([]CompletionTable(nil), tables[:min(5000, len(tables))]...)
	sort.SliceStable(ordered, func(i, j int) bool { return ordered[i].Name < ordered[j].Name })
	if !q.TablePosition {
		for _, ref := range q.References {
			if q.Qualifier != "" && !strings.EqualFold(q.Qualifier, ref.Alias) && !strings.EqualFold(q.Qualifier, ref.Name) {
				continue
			}
			for _, table := range ordered {
				if !strings.EqualFold(table.Name, ref.Name) || ref.Schema != "" && !strings.EqualFold(table.Schema, ref.Schema) {
					continue
				}
				for _, column := range table.Columns[:min(2000, len(table.Columns))] {
					add(column, "字段", table.Name, true)
				}
			}
		}
	}
	if q.Qualifier == "" && !q.TablePosition && q.Quote == 0 {
		words := strings.Fields("SELECT FROM WHERE JOIN LEFT RIGHT INNER OUTER ON AS AND OR NOT NULL IS IN LIKE BETWEEN EXISTS ORDER BY GROUP HAVING LIMIT OFFSET DISTINCT COUNT SUM AVG MIN MAX INSERT INTO VALUES UPDATE SET DELETE CREATE ALTER DROP TABLE VIEW INDEX PRIMARY KEY REFERENCES DEFAULT RETURNING CASE WHEN THEN ELSE END TRUE FALSE CAST COALESCE UNION ALL WITH BEGIN COMMIT ROLLBACK EXPLAIN SHOW DESCRIBE")
		for _, word := range words {
			add(word, "关键字", "SQL", false)
		}
	}
	for _, table := range ordered {
		if q.Qualifier != "" && !strings.EqualFold(q.Qualifier, table.Schema) {
			continue
		}
		add(table.Name, "表", table.Schema, true)
	}
	return result
}

func completionIdentifier(name, dialect string, quote rune) string {
	if quote != 0 {
		end := quote
		if quote == '[' {
			end = ']'
		}
		return string(quote) + strings.ReplaceAll(name, string(end), strings.Repeat(string(end), 2)) + string(end)
	}
	plain := !keywords[strings.ToUpper(name)]
	for i, c := range name {
		plain = plain && (unicode.IsLetter(c) || c == '_' || i > 0 && (unicode.IsDigit(c) || c == '$'))
	}
	// PostgreSQL folds unquoted names to lowercase.
	if dialect == "postgres" && name != strings.ToLower(name) {
		plain = false
	}
	if plain {
		return name
	}
	value, _ := Quote(dialect, name)
	return value
}
