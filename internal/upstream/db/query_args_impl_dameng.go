//go:build superlink_full_drivers || superlink_dameng_driver

package db

import (
	"context"
	"fmt"
)

func (d *DamengDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if d.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, d.conn, "", query, args)
}

func (d *DamengDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if d.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, d.conn, query, args)
}
