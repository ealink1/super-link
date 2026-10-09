package databaseexport

import (
	"context"
	"fmt"
	"github.com/ealink1/super-link/internal/datafile"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"io"
	"strings"
)

func exportTableData(ctx context.Context, source Source, profile domain.Profile, scope string, object domain.Object, output io.Writer, state *Progress, report func()) (int64, error) {
	state.Phase, state.Table, state.TableRows = "导出表数据", object.Name, 0
	report()
	name, err := sqlworkbench.ObjectName(profile.SQLDialect(), object)
	if err != nil {
		return 0, err
	}
	lookup := object.Name
	if object.Schema != "" {
		lookup = name
	}
	info, err := source.TableInfo(ctx, profile.ID, scope, lookup)
	if err != nil {
		return 0, err
	}
	columns := []string{}
	for _, column := range info.Columns {
		if !domain.WritableColumn(column) {
			continue
		}
		q, err := sqlworkbench.Quote(profile.SQLDialect(), column.Name)
		if err != nil {
			return 0, err
		}
		columns = append(columns, q)
	}
	if len(columns) == 0 {
		return 0, fmt.Errorf("表 %s 无可导出的写入列", object.Name)
	}
	encoder, err := datafile.NewEncoder(ctx, output, datafile.Options{Format: "INSERT SQL", Dialect: profile.SQLDialect(), Table: name})
	if err != nil {
		return 0, err
	}
	err = source.StreamQuery(ctx, profile.ID, domain.Execution{Revision: profile.Revision, Scope: scope, Text: "SELECT " + strings.Join(columns, ", ") + " FROM " + name}, &progressConsumer{RowConsumer: encoder, update: func() {
		state.TableRows++
		state.Rows++
	}, report: report})
	if err == nil {
		err = encoder.Finish()
	}
	encoder.Close()
	if err != nil {
		return 0, err
	}

	state.CompletedSteps++
	state.CompletedTables++
	report()
	return encoder.Count, nil
}
