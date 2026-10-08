package sqlworkbench

// TableCatalogQuery reads cached table statistics without scanning table rows.
// Empty means the dialect does not expose this metadata in a supported catalog.
func TableCatalogQuery(kind, scope string) string {
	switch kind {
	case "mysql", "mariadb", "goldendb":
		return "SELECT TABLE_NAME, TABLE_ROWS, DATA_LENGTH, ENGINE, CREATE_TIME, UPDATE_TIME, TABLE_COLLATION, TABLE_COMMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = " + TextLiteral(kind, scope) + " AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME LIMIT 10000"
	}
	return ""
}
