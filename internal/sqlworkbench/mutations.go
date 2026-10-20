package sqlworkbench

import (
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"

	"github.com/ealink1/super-link/internal/domain"
)

// BuildChanges validates metadata identities and creates parameterized mutations.
func BuildChanges(kind string, changes domain.TableChanges, info domain.TableInfo) ([]domain.Statement, error) {
	if len(changes.Rows) == 0 || len(changes.Rows) > 1000 {
		return nil, errors.New("submit between 1 and 1,000 staged rows")
	}
	encoded, err := json.Marshal(changes.Rows)
	if err != nil || len(encoded) > 16<<20 {
		return nil, errors.New("staged changes exceed the 16 MiB budget or contain unsupported values")
	}
	name, err := ObjectName(kind, changes.Object)
	if err != nil {
		return nil, err
	}
	columns := make(map[string]bool, len(info.Columns))
	keys := []string{}
	for _, c := range info.Columns {
		columns[c.Name] = true
		if c.Key == "PRI" {
			keys = append(keys, c.Name)
		}
	}
	statements := make([]domain.Statement, 0, len(changes.Rows))
	for _, row := range changes.Rows {
		for field := range row.Values {
			if !columns[field] {
				return nil, fmt.Errorf("unknown field %q", field)
			}
		}
		for field := range row.Original {
			if !columns[field] {
				return nil, fmt.Errorf("unknown original field %q", field)
			}
		}
		statement, err := buildRow(kind, name, row, info, keys)
		if err != nil {
			return nil, err
		}
		if statement.SQL != "" {
			statements = append(statements, statement)
		}
	}
	if len(statements) == 0 {
		return nil, errors.New("no changed values to submit")
	}
	return statements, nil
}

func placeholder(kind string, position int) string {
	switch kind {
	case "postgres", "kingbase", "highgo", "vastbase", "opengauss", "gaussdb":
		return fmt.Sprintf("$%d", position)
	case "oracle", "dameng":
		return fmt.Sprintf(":%d", position)
	case "sqlserver":
		return fmt.Sprintf("@p%d", position)
	default:
		return "?"
	}
}

func buildRow(kind, name string, row domain.RowChange, info domain.TableInfo, keys []string) (domain.Statement, error) {
	statement := domain.Statement{RequireOne: true}
	bind := func(value any) string {
		statement.Args = append(statement.Args, value)
		return placeholder(kind, len(statement.Args))
	}
	fields, values, sets := []string{}, []string{}, []string{}
	for _, c := range info.Columns {
		value, exists := row.Values[c.Name]
		if !exists {
			continue
		}
		if !domain.WritableColumn(c) {
			return statement, fmt.Errorf("column %q is computed and cannot be assigned", c.Name)
		}
		if row.Kind == "update" {
			old, ok := row.Original[c.Name]
			if !ok {
				return statement, errors.New("original row is incomplete")
			}
			if reflect.DeepEqual(old, value) {
				continue
			}
		}
		q, _ := Quote(kind, c.Name)
		fields = append(fields, q)
		p := bind(value)
		values = append(values, p)
		sets = append(sets, q+" = "+p)
	}
	if row.Kind == "insert" {
		if len(fields) == 0 {
			return statement, errors.New("insert requires an explicit value")
		}
		statement.SQL = "INSERT INTO " + name + " (" + strings.Join(fields, ", ") + ") VALUES (" + strings.Join(values, ", ") + ")"
		return statement, nil
	}
	if row.Kind != "update" && row.Kind != "delete" {
		return statement, errors.New("invalid row change kind")
	}
	if len(keys) == 0 {
		return statement, errors.New("editing existing rows requires a primary key")
	}
	if row.Kind == "update" && len(sets) == 0 {
		return domain.Statement{}, nil
	}
	// Locate by the original primary key, including when the key itself is edited.
	where := []string{}
	for _, c := range info.Columns {
		if c.Key != "PRI" {
			continue
		}
		value, ok := row.Original[c.Name]
		if !ok {
			return statement, fmt.Errorf("original primary key field %q is missing", c.Name)
		}
		value, err := ConvertValue(c.Type, value)
		if err != nil {
			return statement, fmt.Errorf("original field %q cannot be bound: %w", c.Name, err)
		}
		q, _ := Quote(kind, c.Name)
		if value == nil {
			where = append(where, q+" IS NULL")
		} else {
			where = append(where, q+" = "+bind(value))
		}
	}
	if len(where) == 0 {
		return statement, errors.New("row locator is empty")
	}
	if row.Kind == "delete" {
		statement.SQL = "DELETE FROM " + name
	} else {
		statement.SQL = "UPDATE " + name + " SET " + strings.Join(sets, ", ")
	}
	statement.SQL += " WHERE " + strings.Join(where, " AND ")
	return statement, nil
}

// PreviewChanges shows SQL and bound values without executing or persisting them.
func PreviewChanges(statements []domain.Statement) string {
	var output strings.Builder
	for i, s := range statements {
		fmt.Fprintf(&output, "-- %d\n%s;\n", i+1, s.SQL)
		values, _ := json.Marshal(s.Args)
		fmt.Fprintf(&output, "-- parameters: %s\n\n", values)
	}
	return output.String()
}
