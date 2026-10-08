//go:build superlink_full_drivers || superlink_highgo_driver

package db

import (
	"context"
	"fmt"
)

func (h *HighGoDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if h.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	return queryContextWithArgsOnConn(ctx, h.conn, "", query, args)
}

func (h *HighGoDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if h.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, h.conn, query, args)
}
