package runtime

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
	"github.com/ealink1/super-link/internal/upstream/db"
	"github.com/ealink1/super-link/internal/upstream/esconsole"
)

type databaseClient struct {
	database          db.Database
	session           db.StatementExecer
	descriptor        domain.Descriptor
	oracleMode        bool
	defaultSearchPath string
	schemaSelected    bool
}

func openDatabase(ctx context.Context, p domain.Profile, d domain.Descriptor) (Client, error) {
	database, err := db.NewDatabase(d.Key)
	if err != nil {
		return nil, err
	}
	c := &databaseClient{database: database, descriptor: d}
	c.oracleMode = d.Key == "oracle" || d.Key == "oceanbase" && strings.EqualFold(p.Config.OceanBaseProtocol, "oracle")
	success := false
	defer func() {
		if !success {
			_ = c.Close()
		}
	}()
	// Legacy Connect has no context contract; its configured network timeout bounds it.
	// The caller waits for cleanup instead of abandoning a connection goroutine.
	if err = database.Connect(p.Config); err != nil {
		return nil, err
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	if d.Family == domain.SQL && canPin(d.Key) {
		capable := true
		if capability, ok := database.(db.SessionExecerCapability); ok {
			capable = capability.SupportsSessionExecer()
		}
		if provider, ok := database.(db.SessionExecerProvider); ok && capable {
			c.session, err = provider.OpenSessionExecer(ctx)
			if err != nil {
				return nil, fmt.Errorf("open SQL session: %w", err)
			}
			if p.Scope != "" && (d.Key == "oracle" || d.Key == "oceanbase" && strings.EqualFold(p.Config.OceanBaseProtocol, "oracle")) {
				statement := "ALTER SESSION SET CURRENT_SCHEMA = \"" + strings.ReplaceAll(p.Scope, "\"", "\"\"") + "\""
				if _, err = c.session.ExecContext(ctx, statement); err != nil {
					return nil, fmt.Errorf("select Oracle schema: %w", err)
				}
			}
			if p.ReadOnly && !(d.Key == "oceanbase" && p.Config.OceanBaseProtocol == "oracle") {
				if statement := readOnlyStatement(d.Key); statement != "" {
					if _, err = c.session.ExecContext(ctx, statement); err != nil {
						return nil, fmt.Errorf("enable database read-only mode: %w", err)
					}
				}
			}
		}
	}
	success = true
	return c, nil
}

func canPin(key string) bool {
	switch key {
	case "mysql", "goldendb", "postgres", "oracle", "mariadb", "oceanbase", "diros", "starrocks", "sphinx", "sqlserver", "sqlite", "duckdb", "dameng", "kingbase", "highgo", "vastbase", "iris", "cache", "trino":
		return true
	}
	return false
}
func readOnlyStatement(key string) string {
	switch key {
	case "mysql", "goldendb", "mariadb", "oceanbase", "diros", "starrocks":
		return "SET SESSION TRANSACTION READ ONLY"
	case "postgres", "kingbase", "highgo", "vastbase", "opengauss", "gaussdb":
		return "SET default_transaction_read_only = on"
	case "sqlite":
		return "PRAGMA query_only=ON"
	default:
		return ""
	}
}

func (c *databaseClient) Close() error {
	var sessionError error
	if c.session != nil {
		sessionError = c.session.Close()
		c.session = nil
	}
	return errors.Join(sessionError, c.database.Close())
}
func (c *databaseClient) Scopes(ctx context.Context) ([]string, error) {
	db.BindMetadataContext(c.database, ctx)
	defer db.ClearMetadataContext(c.database)
	return c.database.GetDatabases()
}
func (c *databaseClient) Objects(ctx context.Context, scope string) ([]domain.Object, error) {
	if objects, handled, err := c.sqlObjects(ctx, scope); handled {
		return objects, err
	}
	db.BindMetadataContext(c.database, ctx)
	defer db.ClearMetadataContext(c.database)
	var names []string
	var err error
	if c.descriptor.Key == "sqlite" && c.session != nil {
		rows, _, queryErr := c.session.(db.StatementQueryExecer).QueryContext(ctx, "SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
		err = queryErr
		for _, row := range rows {
			names = append(names, fmt.Sprint(row["name"]))
		}
	} else {
		names, err = c.database.GetTables(scope)
	}
	if err != nil {
		return nil, err
	}
	objects := make([]domain.Object, 0, len(names))
	for _, name := range names {
		object := domain.Object{Name: name, Kind: string(c.descriptor.Family), Scope: scope}
		if c.descriptor.Family == domain.SQL {
			object.Kind = "table"
		}
		if c.descriptor.Key == "postgres" {
			object, err = splitPostgresObject(name)
			if err != nil {
				return nil, fmt.Errorf("decode table identity: %w", err)
			}
			object.Scope = scope
		}
		objects = append(objects, object)
	}
	return objects, nil
}
func (c *databaseClient) Schema(ctx context.Context, scope, name string) (string, error) {
	db.BindMetadataContext(c.database, ctx)
	defer db.ClearMetadataContext(c.database)
	if c.descriptor.Key == "sqlite" && c.session != nil {
		query := "SELECT sql FROM sqlite_master WHERE name='" + strings.ReplaceAll(name, "'", "''") + "'"
		rows, _, err := c.session.(db.StatementQueryExecer).QueryContext(ctx, query)
		if err != nil {
			return "", err
		}
		if len(rows) == 0 {
			return "", domain.ErrNotFound
		}
		return fmt.Sprint(rows[0]["sql"]), nil
	}
	if c.descriptor.Key == "postgres" {
		return "-- Complete PostgreSQL CREATE statement is not available from this driver.", errors.New("PostgreSQL DDL retrieval is not implemented; this view cannot be used as a structural backup")
	}
	return c.database.GetCreateStatement(scope, name)
}
func (c *databaseClient) Execute(ctx context.Context, e domain.Execution) ([]domain.Result, error) {
	if err := c.selectSchema(ctx, e.Schema); err != nil {
		return nil, err
	}
	if c.descriptor.Family == domain.Document {
		text, err := mongoCommand(e.Text)
		if err != nil {
			return nil, err
		}
		e.Text = text
	}
	rowLimit := MaxRows
	if e.MaxRows > 0 {
		rowLimit = min(e.MaxRows, MaxRows)
	}
	budget := db.NewRowBudgetWithOptions(db.RowBudgetOptions{PreserveBinary: true, MaxRowsPerResult: rowLimit, MaxTotalRows: rowLimit, MaxTotalBytes: MaxBytes, MaxFieldBytes: MaxFieldBytes})
	ctx = db.ContextWithRowBudget(ctx, budget)
	if len(e.Args) > 0 {
		return c.executeArgs(ctx, e, budget)
	}
	if c.descriptor.Family == domain.Search {
		return c.elasticsearch(ctx, e)
	}
	if e.Write && !(c.descriptor.Family == domain.Message && !strings.HasPrefix(strings.TrimSpace(e.Text), "{")) {
		var count int64
		var err error
		if c.session != nil {
			count, err = c.session.ExecContext(ctx, e.Text)
		} else if execer, ok := c.database.(db.ExecContexter); ok {
			count, err = execer.ExecContext(ctx, e.Text)
		} else {
			return nil, errors.New("driver does not support cancellable writes")
		}
		if err != nil {
			return nil, connectionExecutionError(err)
		}
		return []domain.Result{{RowsAffected: count, Messages: []string{"Statement completed. Writes are not retried automatically."}}}, nil
	}
	var rows []map[string]interface{}
	var columns []string
	var err error
	if c.session != nil {
		if multi, ok := c.session.(db.StatementMultiResultQueryExecer); ok {
			return c.multi(ctx, multi.QueryMultiContext, e.Text, budget)
		}
		query, ok := c.session.(db.StatementQueryExecer)
		if !ok {
			return nil, errors.New("session does not support queries")
		}
		rows, columns, err = query.QueryContext(ctx, e.Text)
	} else {
		if multi, ok := c.database.(db.MultiResultQuerierContext); ok && c.descriptor.Family == domain.SQL && canPin(c.descriptor.Key) {
			return c.multi(ctx, multi.QueryMultiContext, e.Text, budget)
		}
		query, ok := c.database.(db.QueryContexter)
		if !ok {
			return nil, errors.New("driver does not support cancellable queries")
		}
		rows, columns, err = query.QueryContext(ctx, e.Text)
	}
	if err != nil {
		return nil, err
	}
	r := Ordered(rows, columns)
	r.Truncated = r.Truncated || budget.Truncated()
	return []domain.Result{r}, nil
}
func (c *databaseClient) multi(ctx context.Context, query func(context.Context, string) ([]connection.ResultSetData, error), text string, budget *db.RowBudget) ([]domain.Result, error) {
	sets, err := query(ctx, text)
	if err != nil {
		return nil, err
	}
	results := make([]domain.Result, 0, len(sets))
	remaining := MaxRows
	for _, set := range sets {
		r := Ordered(set.Rows, set.Columns)
		r.Messages = set.Messages
		r.Truncated = r.Truncated || set.Truncated || budget.Truncated()
		if len(r.Rows) > remaining {
			r.Rows = r.Rows[:remaining]
			r.Truncated = true
		}
		remaining -= len(r.Rows)
		results = append(results, r)
	}
	return results, nil
}

func (c *databaseClient) elasticsearch(ctx context.Context, e domain.Execution) ([]domain.Result, error) {
	executor, ok := c.database.(db.ElasticsearchConsoleExecutor)
	if !ok {
		return nil, errors.New("driver has no REST console")
	}
	major := 0
	if version, ok := c.database.(db.ElasticsearchServerVersionProvider); ok {
		major = version.ElasticsearchServerMajor()
	}
	batch, err := esconsole.ParseSourceForMajor(e.Text, e.Scope, major)
	if err != nil {
		return nil, err
	}
	if batch.Blocked {
		return nil, errors.New("Elasticsearch request blocked by endpoint policy")
	}
	if batch.ContainsWrite && !e.Write {
		return nil, domain.ErrReadOnly
	}
	result := []domain.Result{}
	for _, request := range batch.Requests {
		response, err := executor.ExecuteElasticsearchConsoleRequest(ctx, db.ElasticsearchConsoleRequest{Method: request.Method, Path: request.Path, Body: request.Body, BodyKind: db.ElasticsearchConsoleBodyKind(request.BodyKind)})
		if err != nil {
			return result, err
		}
		result = append(result, domain.Result{Columns: []domain.Column{{ID: "status", Name: "HTTP status"}, {ID: "body", Name: "JSON response"}}, Rows: [][]any{{response.StatusCode, response.RawBody}}})
		if response.StatusCode >= 400 {
			return result, fmt.Errorf("Elasticsearch HTTP %d for %s %s", response.StatusCode, request.Method, request.Path)
		}
	}
	return result, nil
}
