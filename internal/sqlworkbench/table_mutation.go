package sqlworkbench

import (
	"fmt"
	"github.com/ealink1/super-link/internal/domain"
)

// TableMutationSQL quotes metadata identifiers and preserves each operation's semantics.
func TableMutationSQL(dialect string, object domain.Object, action string) (string, error) {
	if object.Kind != "table" && object.Kind != "sql" {
		return "", fmt.Errorf("此操作仅支持表")
	}
	switch dialect {
	case "mysql", "mariadb", "goldendb", "oceanbase", "postgres", "kingbase", "highgo", "vastbase", "opengauss", "gaussdb", "sqlite", "sqlserver", "oracle", "dm", "duckdb", "clickhouse":
	default:
		return "", fmt.Errorf("此数据库暂不支持此表操作")
	}
	name, err := ObjectName(dialect, object)
	if err != nil {
		return "", err
	}
	switch action {
	case "drop":
		return "DROP TABLE " + name + ";", nil
	case "clear":
		return "DELETE FROM " + name + ";", nil
	case "truncate":
		if dialect == "sqlite" {
			return "", fmt.Errorf("SQLite 不支持截断表，请使用清空表")
		}
		return "TRUNCATE TABLE " + name + ";", nil
	default:
		return "", fmt.Errorf("未知表操作")
	}
}
