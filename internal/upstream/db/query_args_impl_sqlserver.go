//go:build superlink_full_drivers || superlink_sqlserver_driver

package db

import (
	"context"
	"fmt"
)

func (s *SqlServerDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if s.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	// 与 QueryMultiContextWithMessages 的扫描方言保持一致（空串，见
	// scanSQLServerRowsWithMessages），避免日期等列格式在参数化路径下漂移。
	return queryContextWithArgsOnConn(ctx, s.conn, "", query, args)
}

func (s *SqlServerDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if s.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	return execContextWithArgsOnConn(ctx, s.conn, query, args)
}

func (e *sqlServerSessionExecer) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if e == nil || e.conn == nil {
		return nil, nil, fmt.Errorf("连接未打开")
	}
	rows, err := e.conn.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()
	return scanRowsContext(ctx, rows)
}

func (e *sqlServerSessionExecer) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if e == nil || e.conn == nil {
		return 0, fmt.Errorf("连接未打开")
	}
	res, err := e.conn.ExecContext(ctx, query, args...)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}
