//go:build superlink_full_drivers || superlink_sqlite_driver

package db

import (
	"context"
	"fmt"
)

func (s *SQLiteDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if s.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, s.conn, "", query, args)
}

func (s *SQLiteDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if s.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, s.conn, query, args)
}
