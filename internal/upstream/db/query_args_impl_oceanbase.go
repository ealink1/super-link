//go:build superlink_full_drivers || superlink_oceanbase_driver

package db

import (
	"context"
	"errors"
)

// OceanBaseDB 的参数化方法按活动数据库（MySQL/Oracle 协议）转发，
// 避免Oracle 协议连接误走内嵌 MySQLDB 的绑定语义。

func (o *OceanBaseDB) QueryContextWithArgs(ctx context.Context, query string, args []any) ([]map[string]interface{}, []string, error) {
	if target, ok := o.activeDatabase().(QueryArgsContexter); ok {
		return target.QueryContextWithArgs(ctx, query, args)
	}
	return nil, nil, errors.New("当前驱动不支持参数绑定")
}

func (o *OceanBaseDB) ExecContextWithArgs(ctx context.Context, query string, args []any) (int64, error) {
	if target, ok := o.activeDatabase().(ExecArgsContexter); ok {
		return target.ExecContextWithArgs(ctx, query, args)
	}
	return 0, errors.New("当前驱动不支持参数绑定")
}
