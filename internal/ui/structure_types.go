package ui

// Keep parameterized examples editable through the custom option.
func tableColumnTypes(dialect string) []string {
	switch dialect {
	case "mysql", "mariadb", "goldendb", "oceanbase":
		return []string{"INT", "INTEGER", "BIGINT", "SMALLINT", "TINYINT", "VARCHAR(255)", "CHAR(1)", "TEXT", "LONGTEXT", "DECIMAL(10,2)", "FLOAT", "DOUBLE", "BOOLEAN", "DATE", "DATETIME", "TIMESTAMP", "TIME", "JSON", "BLOB"}
	case "sqlite":
		return []string{"INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"}
	default:
		return []string{"INTEGER", "BIGINT", "SMALLINT", "VARCHAR(255)", "CHAR(1)", "TEXT", "NUMERIC(10,2)", "REAL", "DOUBLE PRECISION", "BOOLEAN", "DATE", "TIMESTAMP", "TIMESTAMP WITH TIME ZONE", "TIME", "JSON", "JSONB", "UUID", "BYTEA"}
	}
}
