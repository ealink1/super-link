package sqlworkbench

import "testing"

func TestCreateDatabaseSQLQuotesLiteralNames(t *testing.T) {
	for _, c := range []struct{ dialect, name, want string }{
		{"mysql", "a`b", "CREATE DATABASE `a``b`"},
		{"postgres", `a"b`, `CREATE DATABASE "a""b"`},
		{"sqlserver", "a]b", "CREATE DATABASE [a]]b]"},
		{"mysql", "x; DROP DATABASE y", "CREATE DATABASE `x; DROP DATABASE y`"},
	} {
		got, err := CreateDatabaseSQL(c.dialect, c.name)
		if err != nil || got != c.want {
			t.Fatalf("%s: %q, %v", c.dialect, got, err)
		}
	}
	for _, c := range []struct{ dialect, name string }{{"sqlite", "db"}, {"oracle", "db"}, {"mysql", " "}, {"mysql", "a\x00b"}, {"postgres", "a\nb"}} {
		if _, err := CreateDatabaseSQL(c.dialect, c.name); err == nil {
			t.Fatalf("accepted invalid request: %q %q", c.dialect, c.name)
		}
	}
}

func TestCreateDatabaseOptions(t *testing.T) {
	got, err := CreateDatabaseWithOptionsSQL("mysql", "demo", "utf8mb4", "utf8mb4_unicode_ci")
	if err != nil || got != "CREATE DATABASE `demo` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci" {
		t.Fatal(got, err)
	}
	for _, c := range []struct{ dialect, charset, collation string }{
		{"mysql", "utf8; DROP TABLE x", ""}, {"mysql", "utf8", "x`"}, {"postgres", "utf8", ""},
	} {
		if _, err := CreateDatabaseWithOptionsSQL(c.dialect, "demo", c.charset, c.collation); err == nil {
			t.Fatal("invalid options accepted", c)
		}
	}
}
