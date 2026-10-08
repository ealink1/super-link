//go:build superlink_full_drivers || superlink_vastbase_driver

package db

import (
	"context"
	"fmt"
)

func (v *VastbaseDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if v.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, v.conn, "", query, args)
}

func (v *VastbaseDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if v.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, v.conn, query, args)
}
