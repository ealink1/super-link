package runtime

import (
	"context"
	"errors"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/db"
)

func (c *databaseClient) executeArgs(ctx context.Context, e domain.Execution, budget *db.RowBudget) ([]domain.Result, error) {
	var target any = c.database
	if c.session != nil {
		target = c.session
	}
	if e.Write {
		writer, ok := target.(db.StatementExecArgsExecer)
		if !ok {
			return nil, errors.New("driver has no bound write capability")
		}
		rows, err := writer.ExecContextWithArgs(ctx, e.Text, e.Args)
		if err != nil {
			return nil, connectionExecutionError(err)
		}
		return []domain.Result{{RowsAffected: rows}}, nil
	}
	reader, ok := target.(db.StatementQueryArgsExecer)
	if !ok {
		return nil, errors.New("driver has no bound query capability")
	}
	rows, columns, err := reader.QueryContextWithArgs(ctx, e.Text, e.Args)
	if err != nil {
		return nil, err
	}
	result := Ordered(rows, columns)
	result.Truncated = result.Truncated || budget.Truncated()
	return []domain.Result{result}, nil
}
