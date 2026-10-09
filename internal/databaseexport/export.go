// Package databaseexport streams database table structure and data to a SQL file.
package databaseexport

import (
	"context"
	"errors"
	"fmt"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/filevisibility"
	"github.com/ealink1/super-link/internal/sqlworkbench"
	"io"
	"os"
	"path/filepath"
	"strings"
)

type Source interface {
	Objects(context.Context, string, string) ([]domain.Object, error)
	TableInfo(context.Context, string, string, string) (domain.TableInfo, error)
	StreamQuery(context.Context, string, domain.Execution, domain.RowConsumer) error
}

type Result struct {
	Tables int
	Rows   int64
}

func Save(ctx context.Context, source Source, profile domain.Profile, scope, path string) (Result, error) {
	return SaveWithOptions(ctx, source, profile, scope, path, true)
}

func SaveWithOptions(ctx context.Context, source Source, profile domain.Profile, scope, path string, includeData bool) (Result, error) {
	return SaveWithProgress(ctx, source, profile, scope, path, includeData, nil)
}

func SaveWithProgress(ctx context.Context, source Source, profile domain.Profile, scope, path string, includeData bool, notify func(Progress)) (result Result, err error) {
	state := Progress{Phase: "读取表列表"}
	emit := func() {
		if notify != nil {
			notify(state)
		}
	}
	emit()
	if strings.TrimSpace(scope) == "" || strings.TrimSpace(path) == "" {
		return result, errors.New("请选择数据库与输出文件")
	}
	objects, err := source.Objects(ctx, profile.ID, scope)
	if err != nil {
		return result, err
	}
	if len(objects) > 10000 {
		return result, errors.New("数据库对象超过 10000 个")
	}
	for _, object := range objects {
		if object.Kind == "table" {
			state.Total++
		}
	}
	state.TotalSteps = state.Total
	if includeData {
		state.TotalSteps *= 2
	}
	file, err := os.CreateTemp(filepath.Dir(path), ".superlink-database-*.sql")
	if err != nil {
		return result, err
	}
	defer os.Remove(file.Name())
	defer file.Close()
	output := &limitedWriter{Writer: file, remaining: 8 << 30}
	report := func() { state.Bytes = (8 << 30) - output.remaining; emit() }
	mysql := profile.SQLDialect() == "mysql" || profile.SQLDialect() == "mariadb"
	if mysql {
		db, err := sqlworkbench.Quote(profile.SQLDialect(), scope)
		if err != nil {
			return result, err
		}
		if _, err = fmt.Fprintf(output, "USE %s;\nSET @SUPERLINK_OLD_FK=@@FOREIGN_KEY_CHECKS;\nSET FOREIGN_KEY_CHECKS=0;\n", db); err != nil {
			return result, err
		}
	}
	tables := []domain.Object{}
	for _, object := range objects {
		if object.Kind != "table" {
			continue
		}
		if object.Scope == "" {
			object.Scope = scope
		}
		if object.Scope != scope {
			return result, errors.New("数据库对象范围不匹配")
		}
		if err := ctx.Err(); err != nil {
			return result, err
		}
		name, err := sqlworkbench.ObjectName(profile.SQLDialect(), object)
		if err != nil {
			return result, err
		}
		lookup := object.Name
		if object.Schema != "" {
			lookup = name
		}
		state.Phase, state.Table, state.TableRows = "导出表结构", object.Name, 0
		report()
		info, err := source.TableInfo(ctx, profile.ID, scope, lookup)
		if err != nil {
			return result, err
		}
		if strings.TrimSpace(info.DDL) == "" {
			return result, fmt.Errorf("表 %s 缺少建表结构", object.Name)
		}
		if _, err = fmt.Fprintln(output, strings.TrimRight(strings.TrimSpace(info.DDL), ";")+";\n"); err != nil {
			return result, err
		}
		tables = append(tables, object)
		result.Tables++
		state.CompletedSteps++
		if !includeData {
			state.CompletedTables++
		}
		report()
	}
	if includeData {
		for _, object := range tables {
			rows, exportErr := exportTableData(ctx, source, profile, scope, object, output, &state, report)
			if exportErr != nil {
				return result, exportErr
			}
			result.Rows += rows

		}
	}
	if mysql {
		if _, err = fmt.Fprintln(output, "SET FOREIGN_KEY_CHECKS=@SUPERLINK_OLD_FK;"); err != nil {
			return result, err
		}
	}
	if err = ctx.Err(); err != nil {
		return result, err
	}
	state.Phase, state.Table = "写入文件并校验", ""
	report()
	err = publishExportFile(file, path)
	if err == nil {
		state.Phase = "完成"
		state.Done = true
		report()
	}
	return result, err
}

type limitedWriter struct {
	io.Writer
	remaining int64
}

func (w *limitedWriter) Write(p []byte) (int, error) {
	if int64(len(p)) > w.remaining {
		return 0, errors.New("数据库导出超过 8 GiB")
	}
	n, err := w.Writer.Write(p)
	w.remaining -= int64(n)
	return n, err
}

func publishExportFile(file *os.File, path string) error {
	if err := file.Sync(); err != nil {
		return err
	}
	if err := file.Close(); err != nil {
		return err
	}
	if err := filevisibility.Prepare(file.Name()); err != nil {
		return err
	}
	// Publish only complete exports and never replace an existing destination.
	return os.Link(file.Name(), path)
}
