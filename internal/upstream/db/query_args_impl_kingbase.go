//go:build superlink_full_drivers || superlink_kingbase_driver

package db

import (
	"context"
	"fmt"
)

func (k *KingbaseDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if k.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, k.conn, "", query, args)
}

func (k *KingbaseDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if k.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, k.conn, query, args)
}
