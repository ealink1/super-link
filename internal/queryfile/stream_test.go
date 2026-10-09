package queryfile

import (
	"context"
	"strings"
	"testing"
)

func TestStreamingStatementsRespectStringsCommentsAndDollarQuotes(t *testing.T) {
	for _, c := range []struct {
		dialect, text string
		count         int
	}{
		{"mysql", "-- comment;\nSELECT 'a;''b';/* c; */SELECT `x;y`; # done;\n", 2},
		{"postgres", `SELECT $$a;b$$;SELECT 'a\';SELECT E'a\';b';`, 3},
		{"sqlserver", "SELECT [a;]]b]; SELECT 2;", 2},
	} {
		got := 0
		hash, err := Statements(context.Background(), strings.NewReader(c.text), c.dialect, func(text string, _ int64) error { got++; return nil })
		if err != nil || got != c.count || len(hash) != 64 {
			t.Fatal(c.dialect, got, hash, err)
		}
	}
}
func TestStreamingStatementsHaveNoFileOrStatementSizeLimit(t *testing.T) {
	text := "SELECT '" + strings.Repeat("x", 2<<20) + "'; SELECT 2;"
	count := 0
	_, err := Statements(context.Background(), strings.NewReader(text), "mysql", func(sql string, _ int64) error { count++; return nil })
	if err != nil || count != 2 {
		t.Fatal(count, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Statements(ctx, strings.NewReader(text), "mysql", func(string, int64) error { return nil }); err == nil {
		t.Fatal("cancelled parser continued")
	}
	for _, bad := range []string{"SELECT 'unclosed", "SELECT 1; /* open", "SELECT \x00", "SELECT $$open"} {
		if _, err := Statements(context.Background(), strings.NewReader(bad), "postgres", func(string, int64) error { return nil }); err == nil {
			t.Fatal("accepted malformed SQL")
		}
	}
}
