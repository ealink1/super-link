//go:build superlink_full_drivers || superlink_clickhouse_driver

package db

import (
	"context"
	"fmt"
)

func (c *ClickHouseDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if c.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, c.conn, "", query, args)
}

func (c *ClickHouseDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if c.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, c.conn, query, args)
}
