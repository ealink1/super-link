package sqlworkbench

import (
	"strings"
	"testing"
)

func TestTableCatalogQueryBoundsAndQuotesScope(t *testing.T) {
	for _, kind := range []string{"mysql", "mariadb", "goldendb"} {
		query := TableCatalogQuery(kind, "db'\\; DROP TABLE x")
		if strings.Contains(query, "DROP TABLE") || !strings.Contains(query, "LIMIT 10000") || !strings.Contains(query, "TABLE_ROWS") {
			t.Fatal(query)
		}
	}
	if TableCatalogQuery("sqlite", "main") != "" {
		t.Fatal("unsupported statistics query")
	}
}
