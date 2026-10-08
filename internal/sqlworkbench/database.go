package sqlworkbench

import (
	"fmt"
	"strings"
)

func CanCreateDatabase(dialect string) bool {
	switch dialect {
	case "mysql", "mariadb", "goldendb", "oceanbase", "diros", "starrocks", "postgres", "opengauss", "gaussdb", "highgo", "kingbase", "sqlserver", "clickhouse", "tdengine":
		return true
	}
	return false
}

func CreateDatabaseSQL(dialect, name string) (string, error) {
	if !CanCreateDatabase(dialect) {
		return "", fmt.Errorf("当前数据源不支持新建库")
	}
	name = strings.TrimSpace(name)
	if name == "" || len(name) > 128 || strings.ContainsAny(name, "\x00\r\n") {
		return "", fmt.Errorf("请输入有效的库名（最多 128 字节）")
	}
	quoted, err := Quote(dialect, name)
	if err != nil {
		return "", err
	}
	return "CREATE DATABASE " + quoted, nil
}

func DatabaseCharsetSupported(dialect string) bool {
	switch dialect {
	case "mysql", "mariadb", "goldendb", "oceanbase", "diros":
		return true
	}
	return false
}

func CreateDatabaseWithOptionsSQL(dialect, name, charset, collation string) (string, error) {
	sql, err := CreateDatabaseSQL(dialect, name)
	if err != nil {
		return "", err
	}
	if charset == "" && collation == "" {
		return sql, nil
	}
	if !DatabaseCharsetSupported(dialect) {
		return "", fmt.Errorf("当前数据源不支持字符集与排序规则选项")
	}
	for _, value := range []string{charset, collation} {
		for _, r := range value {
			if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_') {
				return "", fmt.Errorf("无效的字符集或排序规则")
			}
		}
	}
	if charset != "" {
		sql += " CHARACTER SET " + charset
	}
	if collation != "" {
		sql += " COLLATE " + collation
	}
	return sql, nil
}
