package ui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

type queryCompletion struct {
	space      *workspace
	view       *codeCompletion
	key        string
	objects    []domain.Object
	tables     []sqlworkbench.CompletionTable
	loaded     bool
	pending    bool
	attempted  map[string]bool
	cancel     context.CancelFunc
	sequence   uint64
	retryAfter time.Time
}

type completionMetadata struct {
	objects []domain.Object
	tables  []sqlworkbench.CompletionTable
}

func (s *workspace) setupCompletion() {
	q := &queryCompletion{space: s, attempted: map[string]bool{}}
	q.view = &codeCompletion{entry: s.code, request: q.update}
	s.code.completion, s.completion = q.view, q
}

func (q *queryCompletion) reset() {
	q.sequence++
	if q.cancel != nil {
		q.cancel()
		q.cancel = nil
	}
	q.view.hide()
	q.key, q.objects, q.tables = "", nil, nil
	q.loaded, q.pending = false, false
	q.attempted = map[string]bool{}
	q.retryAfter = time.Time{}
}

func (q *queryCompletion) contextKey() string {
	s := q.space
	return fmt.Sprintf("%s\x00%d\x00%s\x00%s", s.profile.ID, s.profile.Revision, s.scope.Text, s.schemaName)
}

func (q *queryCompletion) update(manual bool) {
	s := q.space
	if s.closed || s.busy || s.owner.tabs.Selected() != s.item || !q.view.requested {
		q.view.hide()
		return
	}
	key := q.contextKey()
	if key != q.key {
		q.reset()
		q.key, q.view.requested = key, true
	}
	query := sqlworkbench.AnalyzeCompletion(s.editor.Text, s.editor.CursorTextOffset())
	if !query.Active || !manual && query.Prefix == "" && query.Qualifier == "" && !query.TablePosition {
		q.view.hide()
		return
	}
	q.view.show(query, sqlworkbench.Complete(query, q.tables, s.profile.SQLDialect()))
	if !q.pending && (manual || time.Now().After(q.retryAfter)) {
		q.load(query)
	}
}

func (q *queryCompletion) seed(objects []domain.Object) {
	q.reset()
	q.key = q.contextKey()
	q.objects, q.tables = completionTables(objects, q.space.profile.SQLDialect(), q.space.scope.Text)
	q.loaded = true
}

func (q *queryCompletion) load(query sqlworkbench.CompletionQuery) {
	s := q.space
	refs := []domain.Object{}
	if q.loaded {
		for _, object := range q.objects {
			if len(refs) >= 8 {
				break
			}
			key := object.Schema + "\x00" + object.Name
			if !q.attempted[key] && completionNeedsColumns(query, object, s.schemaName, s.scope.Text) {
				q.attempted[key] = true
				refs = append(refs, object)
			}
		}
		if len(refs) == 0 {
			return
		}
	}
	profile, scope, schema := s.profile, s.scope.Text, s.schemaName
	loaded, objects := q.loaded, q.objects
	tables := append([]sqlworkbench.CompletionTable(nil), q.tables...)
	q.pending = true
	sequence := q.sequence
	owner := s.owner
	owner.jobs.runWithCancel(&q.cancel, func(ctx context.Context) (any, error) {
		timer := time.NewTimer(200 * time.Millisecond)
		defer timer.Stop()
		select {
		case <-timer.C:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
		ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
		defer cancel()
		current, err := owner.Profiles.Get(ctx, profile.ID)
		if err != nil {
			return nil, err
		}
		if current.Revision != profile.Revision {
			return nil, domain.ErrConflict
		}
		if !loaded {
			all, err := owner.Engine.Objects(ctx, profile.ID, scope)
			if err != nil {
				return nil, err
			}
			objects, tables = completionTables(all, profile.SQLDialect(), scope)
			current, err = owner.Profiles.Get(ctx, profile.ID)
			if err != nil || current.Revision != profile.Revision {
				return nil, domain.ErrConflict
			}
			return completionMetadata{objects: objects, tables: tables}, nil
		}
		remaining := 20000
		for _, table := range tables {
			remaining -= len(table.Columns)
		}
		for _, object := range refs {
			if remaining <= 0 {
				break
			}
			name := object.Name
			if object.Schema != "" {
				name, err = sqlworkbench.ObjectName(profile.SQLDialect(), object)
				if err != nil {
					continue
				}
			}
			info, err := owner.Engine.TableInfo(ctx, profile.ID, scope, name)
			if err != nil {
				continue
			}
			for i, table := range tables {
				if table.Name == object.Name && (table.Schema == object.Schema || object.Schema == "" && table.Schema == scope) {
					columns := make([]string, 0, min(2000, len(info.Columns)))
					for _, column := range info.Columns[:min(remaining, 2000, len(info.Columns))] {
						if len(column.Name) <= 512 {
							columns = append(columns, column.Name)
						}
					}
					tables[i].Columns = columns
					remaining -= len(columns)
				}
			}
		}
		current, err = owner.Profiles.Get(ctx, profile.ID)
		if err != nil || current.Revision != profile.Revision {
			return nil, domain.ErrConflict
		}
		return completionMetadata{objects: objects, tables: tables}, nil
	}, func(value any, err error) {
		if s.closed || sequence != q.sequence || q.key != q.contextKey() || schema != s.schemaName {
			return
		}
		q.pending, q.cancel = false, nil
		if err != nil {
			q.retryAfter = time.Now().Add(5 * time.Second)
			return
		}
		metadata := value.(completionMetadata)
		q.objects, q.tables, q.loaded = metadata.objects, metadata.tables, true
		if q.view.popup != nil && !q.view.popup.Visible() {
			q.view.hide()
		}
		if q.view.requested {
			q.update(false)
		}
	})
}

func completionTables(all []domain.Object, dialect, scope string) ([]domain.Object, []sqlworkbench.CompletionTable) {
	objects := make([]domain.Object, 0, min(5000, len(all)))
	tables := make([]sqlworkbench.CompletionTable, 0, cap(objects))
	for _, object := range all {
		if len(objects) >= 5000 {
			break
		}
		if object.Kind != "table" && object.Kind != "view" || len(object.Name) > 512 || len(object.Schema) > 512 {
			continue
		}
		schema := object.Schema
		if schema == "" && (dialect == "mysql" || dialect == "mariadb") {
			schema = scope
		}
		objects = append(objects, object)
		tables = append(tables, sqlworkbench.CompletionTable{Schema: schema, Name: object.Name})
	}
	return objects, tables
}

func completionNeedsColumns(query sqlworkbench.CompletionQuery, object domain.Object, schema, scope string) bool {
	for _, ref := range query.References {
		if strings.EqualFold(ref.Name, object.Name) && (ref.Schema == "" && (schema == "" || object.Schema == "" || object.Schema == schema) || strings.EqualFold(ref.Schema, object.Schema) || strings.EqualFold(ref.Schema, scope)) {
			return true
		}
	}
	return false
}
