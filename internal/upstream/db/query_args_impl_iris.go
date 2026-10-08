//go:build superlink_full_drivers || superlink_iris_driver || superlink_cache_driver

package db

import (
	"context"
	"fmt"
)

func (i *IrisDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if i.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, i.conn, "", query, args)
}

func (i *IrisDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if i.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, i.conn, query, args)
}
