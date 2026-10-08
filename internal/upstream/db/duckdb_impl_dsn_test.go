//go:build superlink_full_drivers || superlink_duckdb_driver

package db

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/ealink1/super-link/internal/upstream/connection"
)

// TestDuckDBConnectDSNAllowUnsignedExtensions 钉住 Connect 的 DSN 构造契约：
// 所有形态都追加 allow_unsigned_extensions=true（本地编译扩展依赖该能力），
// 且不破坏原路径语义；用户显式给出 false 时保留用户值。
func TestDuckDBConnectDSNAllowUnsignedExtensions(t *testing.T) {
	filePath := filepath.Join(t.TempDir(), "dsn.db")

	cases := []struct {
		name     string
		host     string
		expected string
	}{
		{name: "memory", host: ":memory:", expected: "true"},
		{
			name:     "file path",
			host:     filePath,
			expected: "true",
		},
		{
			name:     "memory with existing params",
			host:     ":memory:?allow_unsigned_extensions=true",
			expected: "true",
		},
		{
			name:     "user override wins",
			host:     ":memory:?allow_unsigned_extensions=false",
			expected: "false",
		},
	}

	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			d := &DuckDB{}
			if err := d.Connect(connection.ConnectionConfig{Host: tc.host}); err != nil {
				t.Fatalf("connect: %v", err)
			}
			defer func() { _ = d.Close() }()

			var setting string
			if err := d.conn.QueryRowContext(context.Background(),
				"SELECT current_setting('allow_unsigned_extensions')").Scan(&setting); err != nil {
				t.Fatalf("read setting: %v", err)
			}
			if setting != tc.expected {
				t.Fatalf("allow_unsigned_extensions = %q, want %q (dsn=%q)", setting, tc.expected, tc.host)
			}
		})
	}
}

// TestDuckDBConnectDSNFileStillQueryable 确认追加查询参数后文件库可正常打开写入。
func TestDuckDBConnectDSNFileStillQueryable(t *testing.T) {
	path := filepath.Join(t.TempDir(), "plain.duckdb")
	d := &DuckDB{}
	if err := d.Connect(connection.ConnectionConfig{Host: path}); err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer func() { _ = d.Close() }()

	if _, err := d.conn.ExecContext(context.Background(), "CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (7);"); err != nil {
		t.Fatalf("exec: %v", err)
	}
	var v int
	if err := d.conn.QueryRowContext(context.Background(), "SELECT v FROM t").Scan(&v); err != nil {
		t.Fatalf("query: %v", err)
	}
	if v != 7 {
		t.Fatalf("v = %d, want 7", v)
	}
}
