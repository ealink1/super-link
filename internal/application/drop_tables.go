package application

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

// DropTables binds a single confirmation to the exact ordered list. Statements
// execute separately because not all drivers accept multi-statement SQL. DDL
// can commit independently; return completed targets even on partial failure.
func (e *Engine) DropTables(ctx context.Context, id string, revision int64, objects []domain.Object, confirmation string, progress func(int, int)) ([]domain.Object, error) {
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return nil, err
	}
	if revision != p.Revision {
		return nil, domain.ErrConflict
	}
	if p.ReadOnly || p.Config.Protection.RestrictStructureEdit {
		return nil, domain.ErrReadOnly
	}
	if len(objects) == 0 || len(objects) > 1000 {
		return nil, fmt.Errorf("请选择 1–1000 张表")
	}
	statements := make([]string, 0, len(objects))
	seen := make(map[domain.Object]bool, len(objects))
	for _, object := range objects {
		if object.Scope != objects[0].Scope || seen[object] {
			return nil, fmt.Errorf("删除目标必须属于同一个数据库且不能重复")
		}
		seen[object] = true
		sql, err := sqlworkbench.TableMutationSQL(p.SQLDialect(), object, "drop")
		if err != nil {
			return nil, err
		}
		statements = append(statements, sql)
	}
	batch := domain.Execution{Revision: revision, Scope: objects[0].Scope, Text: strings.Join(statements, "\n"), Action: "structure", Write: true, Confirmation: confirmation}
	if len(batch.Text) > 1<<20 {
		return nil, fmt.Errorf("删除列表超过 1 MiB")
	}
	if err = e.authorize(p, batch); err != nil {
		return nil, err
	}
	completed := make([]domain.Object, 0, len(objects))
	for i, sql := range statements {
		if err = ctx.Err(); err != nil {
			return completed, err
		}
		request := domain.Execution{Revision: revision, Scope: objects[i].Scope, Schema: objects[i].Schema, Text: sql, Action: "structure", Write: true, MaxRows: 1}
		// The batch grant permits this exact statement; Execute still enforces live
		// profile policy, revision and its own single-use statement authorization.
		_, err = e.Execute(ctx, id, request)
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			request.Confirmation = required.Fingerprint
			_, err = e.Execute(ctx, id, request)
		}
		if err != nil {
			return completed, fmt.Errorf("删除表 %s 失败（已完成 %d / %d）：%w", objects[i].Name, len(completed), len(objects), err)
		}
		completed = append(completed, objects[i])
		if progress != nil {
			progress(len(completed), len(objects))
		}
	}
	return completed, nil
}
