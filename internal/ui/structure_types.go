package ui

import (
	"regexp"
	"strings"
)

// tableColumnTypes lists base types; parameters live in the separate 长度 column.
func tableColumnTypes(dialect string) []string {
	switch dialect {
	case "mysql", "mariadb", "goldendb", "oceanbase":
		return []string{"INT", "INTEGER", "BIGINT", "SMALLINT", "TINYINT", "VARCHAR", "CHAR", "TEXT", "LONGTEXT", "DECIMAL", "FLOAT", "DOUBLE", "BOOLEAN", "DATE", "DATETIME", "TIMESTAMP", "TIME", "JSON", "BLOB"}
	case "sqlite":
		return []string{"INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"}
	default:
		return []string{"INTEGER", "BIGINT", "SMALLINT", "VARCHAR", "CHAR", "TEXT", "NUMERIC", "REAL", "DOUBLE PRECISION", "BOOLEAN", "DATE", "TIMESTAMP", "TIMESTAMP WITH TIME ZONE", "TIME", "JSON", "JSONB", "UUID", "BYTEA"}
	}
}

// splitColumnType separates "VARCHAR(255)" into its base, parameters and any
// trailing qualifier such as " UNSIGNED".
func splitColumnType(value string) (base, args, suffix string) {
	text := strings.TrimSpace(value)
	open := strings.Index(text, "(")
	if open < 0 {
		return text, "", ""
	}
	end := strings.Index(text[open:], ")")
	if end < 0 {
		return text, "", ""
	}
	end += open
	return strings.TrimSpace(text[:open]), strings.TrimSpace(text[open+1 : end]), strings.TrimSpace(text[end+1:])
}

// joinColumnType rebuilds the SQL type from its parts.
func joinColumnType(base, args, suffix string) string {
	text := strings.TrimSpace(base)
	if text == "" {
		return ""
	}
	if params := strings.TrimSpace(args); params != "" {
		text += "(" + params + ")"
	}
	if tail := strings.TrimSpace(suffix); tail != "" {
		text += " " + tail
	}
	return text
}

var lengthArgsPattern = regexp.MustCompile(`^\d{1,4}(,\s*\d{1,4})?$`)

// validColumnTypeArgs accepts an empty value, "100" or "10,2".
func validColumnTypeArgs(args string) bool {
	text := strings.TrimSpace(args)
	return text == "" || lengthArgsPattern.MatchString(text)
}

func columnTypeAcceptsLength(base string) bool {
	switch strings.ToUpper(strings.TrimSpace(base)) {
	case "VARCHAR", "VARCHAR2", "NVARCHAR", "CHAR", "NCHAR", "BINARY", "VARBINARY",
		"DECIMAL", "NUMERIC", "NUMBER", "BIT", "TIME", "DATETIME", "TIMESTAMP":
		return true
	}
	return false
}

// columnTypeDefaultArgs seeds the length cell when a parameterised type is picked.
func columnTypeDefaultArgs(base string) string {
	switch strings.ToUpper(strings.TrimSpace(base)) {
	case "VARCHAR", "VARCHAR2", "NVARCHAR", "VARBINARY":
		return "255"
	case "CHAR", "NCHAR", "BINARY", "BIT":
		return "1"
	case "DECIMAL", "NUMERIC", "NUMBER":
		return "10,2"
	}
	return ""
}

func columnTypeKeepsSuffix(base string) bool {
	switch strings.ToUpper(strings.TrimSpace(base)) {
	case "INT", "INTEGER", "BIGINT", "SMALLINT", "TINYINT", "MEDIUMINT":
		return true
	}
	return false
}
