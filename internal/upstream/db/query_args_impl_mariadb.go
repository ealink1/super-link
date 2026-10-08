//go:build superlink_full_drivers || superlink_mariadb_driver

package db

import (
	"context"
	"fmt"
)

func (m *MariaDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if m.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, m.conn, "mariadb", query, args)
}

func (m *MariaDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if m.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, m.conn, query, args)
}
