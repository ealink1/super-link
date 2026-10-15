package sqlworkbench

import (
	"strings"
	"testing"
)

func completionAtMarker(source string) (string, CompletionQuery) {
	parts := strings.SplitN(source, "|", 2)
	text := strings.Join(parts, "")
	return text, AnalyzeCompletion(text, len([]rune(parts[0])))
}

func TestCompletionUsesSQLContextAndRuneRanges(t *testing.T) {
	tables := []CompletionTable{
		{Schema: "public", Name: "users", Columns: []string{"id", "name", "昵称", "order"}},
		{Schema: "public", Name: "user_logs", Columns: []string{"event"}},
		{Schema: "other", Name: "payments", Columns: []string{"amount"}},
	}
	for _, tc := range []struct{ source, want, kind string }{
		{"se|", "select", "关键字"},
		{"SELECT * FROM us|", "users", "表"},
		{"SELECT * FROM public.us|", "users", "表"},
		{"SELECT u.na| FROM users u", "name", "字段"},
		{"SELECT u.昵| FROM public.users AS u", "昵称", "字段"},
		{"SELECT na| FROM users", "name", "字段"},
		{"SELECT u.or| FROM users u", `"order"`, "字段"},
		{"SELECT \"na|\" FROM users", `"name"`, "字段"},
		{"SELECT * FROM users u JOIN payments p ON p.am|", "amount", "字段"},
	} {
		t.Run(tc.source, func(t *testing.T) {
			text, query := completionAtMarker(tc.source)
			items := Complete(query, tables, "postgres")
			found := false
			for _, item := range items {
				if item.Insert == tc.want && item.Kind == tc.kind {
					found = true
					out := []rune(text)
					replaced := string(out[:query.Start]) + item.Insert + string(out[query.End:])
					if strings.Contains(replaced, `"name""`) {
						t.Fatal("completion duplicated closing identifier quote")
					}
				}
			}
			if !found {
				t.Fatalf("missing %q (%s) for %+v: %+v", tc.want, tc.kind, query, items)
			}
			if query.TablePosition {
				for _, item := range items {
					if item.Kind != "表" {
						t.Fatal("table position mixed in fields or keywords", items)
					}
				}
			}
		})
	}
}

func TestCompletionSuppressesStringsCommentsAndParameters(t *testing.T) {
	for _, source := range []string{
		"SELECT 'se|", "SELECT 'it''s se| here'", "-- SELECT se|", "/* outer /* nested */ se| */",
		"SELECT $body$ SELECT se| $body$", "SELECT $$ se|", "SELECT :na|", "SELECT @na|", "SELECT ${na|}",
	} {
		_, q := completionAtMarker(source)
		if q.Active {
			t.Errorf("completion active inside non-code: %q (%+v)", source, q)
		}
	}
	_, q := completionAtMarker("-- comment\nse|")
	if !q.Active || q.Prefix != "se" {
		t.Fatal("line comment suppressed subsequent SQL", q)
	}
}

func TestCompletionReplacesWholeTokenAndDoesNotLeakOtherStatements(t *testing.T) {
	text, q := completionAtMarker("SELECT '中文'; SELECT u.na|me FROM users u; SELECT * FROM payments p")
	if string([]rune(text)[q.Start:q.End]) != "name" || q.Prefix != "na" || len(q.References) != 1 || q.References[0].Name != "users" {
		t.Fatal("completion range or statement boundary incorrect", q)
	}
	q = AnalyzeCompletion(strings.Repeat(" ", 256<<10)+"se", 2)
	if q.Active {
		t.Fatal("large scripts did unbounded completion work")
	}
}

func TestCompletionQuotesMetadataAndLimitsResults(t *testing.T) {
	for _, tc := range []struct{ dialect, quote, want string }{
		{"mysql", "", "`order`"}, {"sqlserver", "", "[order]"}, {"postgres", "", `"order"`},
	} {
		_, q := completionAtMarker("SELECT * FROM or|")
		items := Complete(q, []CompletionTable{{Name: "order"}}, tc.dialect)
		if len(items) != 1 || items[0].Insert != tc.want {
			t.Fatal("reserved identifier was not quoted", tc, items)
		}
	}
	tables := make([]CompletionTable, 6000)
	for i := range tables {
		tables[i].Name = strings.Repeat("x", i%100+1)
	}
	_, q := completionAtMarker("SELECT * FROM |")
	if items := Complete(q, tables, "mysql"); len(items) != 60 {
		t.Fatal("completion result limit changed", len(items))
	}
}
