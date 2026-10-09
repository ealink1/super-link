package application

import (
	"encoding/json"
	"errors"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/sqlparam"
)

func prepareExecution(p domain.Profile, request domain.Execution) (domain.Execution, error) {
	if request.Args != nil {
		return request, errors.New("direct positional arguments are not accepted; use named parameters")
	}
	if len(request.Parameters) > 1024 {
		return request, errors.New("query exceeds 1,024 named parameters")
	}
	d, err := domain.Resolve(p.Config.Type)
	if err != nil {
		return request, err
	}
	if d.Family != domain.SQL {
		if len(request.Parameters) == 0 {
			return request, nil
		}
		return request, errors.New("named parameters require a SQL data source")
	}
	if len(request.Text) > 1<<20 {
		return request, errors.New("command exceeds 1 MiB")
	}
	for _, value := range request.Parameters {
		if list, ok := value.Value.([]any); ok && len(list) > 10000 {
			return request, errors.New("parameter list exceeds 10,000 items")
		}
		if v, ok := value.Value.(string); ok && len(v) > 1<<20 {
			return request, errors.New("parameter exceeds 1 MiB")
		}
	}
	count := 0
	for _, span := range sqlparam.Scan(request.Text, sqlparam.OptionsForDBType(p.SQLDialect())) {
		count++
		if list, ok := request.Parameters[span.Name].Value.([]any); ok && span.Template == "" {
			count += max(0, len(list)-1)
		}
		if count > 10000 {
			return request, errors.New("bound query exceeds 10,000 arguments")
		}
	}
	raw, err := json.Marshal(request.Parameters)
	if err != nil || len(raw) > 16<<20 {
		return request, errors.New("invalid parameters or content larger than 16 MiB")
	}
	values := map[string]sqlparam.TypedValue{}
	for name, value := range request.Parameters {
		values[name] = sqlparam.TypedValue{Type: value.Type, Value: value.Value}
	}
	bound, err := sqlparam.Bind(request.Text, p.SQLDialect(), values)
	if err != nil {
		return request, err
	}
	if len(bound.SQL) > 1<<20 || len(bound.Args) > 10000 {
		return request, errors.New("bound query exceeds statement or argument limits")
	}
	if raw, err = json.Marshal(bound.Args); err != nil || len(raw) > 16<<20 {
		return request, errors.New("bound values must be finite, valid and at most 16 MiB")
	}
	request.Text, request.Args = bound.SQL, bound.Args
	return request, nil
}

func ClassifyProfile(p domain.Profile, request domain.Execution) (bool, error) {
	prepared, err := prepareExecution(p, request)
	if err != nil {
		return false, err
	}
	d, err := domain.Resolve(p.Config.Type)
	if err != nil {
		return false, err
	}
	if d.Family == domain.SQL {
		d.Key = p.SQLDialect()
	}
	return Classify(d, prepared)
}

func validateStructureScript(request domain.Execution, dialect string) error {
	if request.Action != "structure" {
		return nil
	}
	tokens, err := sqlTokens(request.Text, dialect)
	if err != nil {
		return err
	}
	start := true
	for _, word := range tokens {
		if word == ";" {
			start = true
			continue
		}
		if start && word != "CREATE" && word != "ALTER" && word != "DROP" && word != "COMMENT" && word != "RENAME" && word != "TRUNCATE" {
			return errors.New("structure action accepts DDL statements only")
		}
		start = false
	}
	return nil
}
