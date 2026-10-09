package sqlworkbench

import (
	"github.com/ealink1/super-link/internal/domain"
	"testing"
)

func TestTableMutationSQL(t *testing.T) {
	for _, tc := range []struct{ dialect, action, want string }{
		{"mysql", "drop", "DROP TABLE `db`.`a``b`;"},
		{"postgres", "clear", "DELETE FROM \"a`b\";"},
		{"sqlserver", "truncate", "TRUNCATE TABLE [a`b];"},
		{"sqlite", "clear", "DELETE FROM \"a`b\";"},
	} {
		got, err := TableMutationSQL(tc.dialect, domain.Object{Kind: "table", Scope: "db", Name: "a`b"}, tc.action)
		if err != nil || got != tc.want {
			t.Fatalf("%s %s: %q %v", tc.dialect, tc.action, got, err)
		}
	}
	for _, tc := range []struct{ dialect, action, kind, name string }{
		{"sqlite", "truncate", "table", "items"}, {"redis", "drop", "table", "items"},
		{"mysql", "drop", "view", "items"}, {"mysql", "drop", "table", ""}, {"mysql", "unknown", "table", "items"},
	} {
		if _, err := TableMutationSQL(tc.dialect, domain.Object{Kind: tc.kind, Name: tc.name}, tc.action); err == nil {
			t.Fatalf("accepted unsupported operation: %+v", tc)
		}
	}
}
