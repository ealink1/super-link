package application

import (
	"context"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

type TableCatalog struct {
	Objects         []domain.Object
	Statistics      map[string][]any
	StatisticsError error
	Truncated       bool
}

// TableCatalog retains the object list when optional catalog statistics fail.
func (e *Engine) TableCatalog(ctx context.Context, profile domain.Profile, scope string) (TableCatalog, error) {
	objects, err := e.Objects(ctx, profile.ID, scope)
	if err != nil {
		return TableCatalog{}, err
	}
	catalog := TableCatalog{Objects: objects, Statistics: map[string][]any{}}
	query := sqlworkbench.TableCatalogQuery(profile.SQLDialect(), scope)
	if query == "" {
		return catalog, nil
	}
	results, err := e.Execute(ctx, profile.ID, domain.Execution{Revision: profile.Revision, Scope: scope, Text: query, MaxRows: 10000})
	if err != nil {
		catalog.StatisticsError = err
		return catalog, nil
	}
	for _, result := range results {
		catalog.Truncated = catalog.Truncated || result.Truncated
		for _, row := range result.Rows {
			if len(row) != 8 {
				continue
			}
			var name string
			switch value := row[0].(type) {
			case string:
				name = value
			case []byte:
				name = string(value)
			}
			if name != "" {
				catalog.Statistics[name] = append([]any(nil), row[1:]...)
			}
		}
	}
	return catalog, nil
}
