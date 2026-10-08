//go:build superlink_full_drivers || superlink_trino_driver

package db

import (
	"context"
	"fmt"
)

func (t *TrinoDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if t.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, t.conn, "", query, args)
}

func (t *TrinoDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if t.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, t.conn, query, args)
}
