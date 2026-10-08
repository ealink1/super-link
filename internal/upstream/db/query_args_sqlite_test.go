//go:build superlink_full_drivers || superlink_sqlite_driver

package db

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"github.com/ealink1/super-link/internal/upstream/connection"
	"github.com/ealink1/super-link/internal/upstream/sqlparam"
)

// newArgsTestSQLiteDB 打开内存 SQLite 并建好测试表，验证绑定执行端到端语义。
func newArgsTestSQLiteDB(t *testing.T) *SQLiteDB {
	t.Helper()
	client := &SQLiteDB{}
	if err := client.Connect(connection.ConnectionConfig{Type: "sqlite", Host: ":memory:"}); err != nil {
		t.Fatalf("连接内存 SQLite 失败: %v", err)
	}
	t.Cleanup(func() { _ = client.Close() })
	ctx := context.Background()
	setup := []string{
		"CREATE TABLE bind_items (id INTEGER PRIMARY KEY, name TEXT, price REAL, active INTEGER, created_at DATETIME)",
		"INSERT INTO bind_items (id, name, price, active, created_at) VALUES (1, 'alice', 9.5, 1, '2026-08-01 10:00:00')",
		"INSERT INTO bind_items (id, name, price, active, created_at) VALUES (2, 'bob', 3, 0, '2026-08-02 10:00:00')",
		"INSERT INTO bind_items (id, name, price, active, created_at) VALUES (3, 'carol', 12, 1, '2026-08-03 10:00:00')",
	}
	for _, stmt := range setup {
		if _, err := client.ExecContext(ctx, stmt); err != nil {
			t.Fatalf("初始化表失败: %v", err)
		}
	}
	return client
}

func TestSQLiteQueryContextWithArgsBindsNamedParameters(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	ctx := context.Background()

	bound, err := sqlparam.Bind(
		"SELECT id FROM bind_items WHERE name = :name AND active = :active ORDER BY id",
		"sqlite",
		map[string]sqlparam.TypedValue{
			"name":   {Type: sqlparam.TypeString, Value: "bob"},
			"active": {Type: sqlparam.TypeBoolean, Value: false},
		},
	)
	if err != nil {
		t.Fatalf("Bind 返回错误: %v", err)
	}
	rows, columns, err := client.QueryContextWithArgs(ctx, bound.SQL, bound.Args)
	if err != nil {
		t.Fatalf("QueryContextWithArgs 返回错误: %v", err)
	}
	if len(rows) != 1 || rows[0]["id"] != int64(2) {
		t.Fatalf("布尔绑定查询结果异常: %#v", rows)
	}
	if len(columns) != 1 || columns[0] != "id" {
		t.Fatalf("列信息异常: %#v", columns)
	}
}

func TestSQLiteExecContextWithArgsBindsInsert(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	ctx := context.Background()

	bound, err := sqlparam.Bind(
		"INSERT INTO bind_items (id, name, price, active, created_at) VALUES (:id, :name, :price, :active, :created_at)",
		"sqlite",
		map[string]sqlparam.TypedValue{
			"id":         {Type: sqlparam.TypeNumber, Value: float64(4)},
			"name":       {Type: sqlparam.TypeString, Value: "dave"},
			"price":      {Type: sqlparam.TypeNumber, Value: 7.25},
			"active":     {Type: sqlparam.TypeBoolean, Value: true},
			"created_at": {Type: sqlparam.TypeDatetime, Value: "2026-09-01 08:30:00"},
		},
	)
	if err != nil {
		t.Fatalf("Bind 返回错误: %v", err)
	}
	affected, err := client.ExecContextWithArgs(ctx, bound.SQL, bound.Args)
	if err != nil {
		t.Fatalf("ExecContextWithArgs 返回错误: %v", err)
	}
	if affected != 1 {
		t.Fatalf("affected 行数异常: %d", affected)
	}
	rows, _, err := client.QueryContext(ctx, "SELECT name, price, active, created_at FROM bind_items WHERE id = 4")
	if err != nil {
		t.Fatalf("回读失败: %v", err)
	}
	if len(rows) != 1 {
		t.Fatalf("回读行数异常: %#v", rows)
	}
	row := rows[0]
	if row["name"] != "dave" {
		t.Fatalf("name 绑定异常: %#v", row["name"])
	}
	if price, ok := row["price"].(float64); !ok || price != 7.25 {
		t.Fatalf("price 绑定异常: %#v (%T)", row["price"], row["price"])
	}
	if active, ok := row["active"].(int64); !ok || active != 1 {
		t.Fatalf("布尔绑定应落库为 1: %#v (%T)", row["active"], row["active"])
	}
	if createdAt, ok := row["created_at"].(string); !ok || !strings.HasPrefix(createdAt, "2026-09-01T08:30:00") {
		// 扫描显示层会把 time.Time 格式化为 RFC3339 字符串；绑定正确性以
		// 回读值能还原写入时刻为准。
		t.Fatalf("日期时间绑定回读异常: %#v (%T)", row["created_at"], row["created_at"])
	}
}

func TestSQLiteQueryContextWithArgsBindsListAndNull(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	ctx := context.Background()

	bound, err := sqlparam.Bind(
		"SELECT id FROM bind_items WHERE id IN (:ids) OR name IS :missing ORDER BY id",
		"sqlite",
		map[string]sqlparam.TypedValue{
			"ids":     {Type: sqlparam.TypeList, Value: []any{float64(1), float64(3)}},
			"missing": {Type: sqlparam.TypeNull},
		},
	)
	if err != nil {
		t.Fatalf("Bind 返回错误: %v", err)
	}
	if bound.SQL != "SELECT id FROM bind_items WHERE id IN (?,?) OR name IS ? ORDER BY id" {
		t.Fatalf("列表展开重写异常: %q", bound.SQL)
	}
	rows, _, err := client.QueryContextWithArgs(ctx, bound.SQL, bound.Args)
	if err != nil {
		t.Fatalf("QueryContextWithArgs 返回错误: %v", err)
	}
	// name IS ?（NULL 绑定）在 SQL 里恒为假，命中的只有 IN 列表的 1 和 3。
	if len(rows) != 2 || rows[0]["id"] != int64(1) || rows[1]["id"] != int64(3) {
		t.Fatalf("列表绑定查询结果异常: %#v", rows)
	}
}

func TestSQLiteSameParameterReusedAcrossOccurrences(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	ctx := context.Background()

	// 同名参数填一次、两处占位符同时生效：>= :day AND < :day 恒假（0 行），
	// 若两次出现被绑定成不同值则可能误命中。
	bound, err := sqlparam.Bind(
		"SELECT id FROM bind_items WHERE created_at >= :day AND created_at < :day ORDER BY id",
		"sqlite",
		map[string]sqlparam.TypedValue{
			"day": {Type: sqlparam.TypeDatetime, Value: "2026-08-02 10:00:00"},
		},
	)
	if err != nil {
		t.Fatalf("Bind 返回错误: %v", err)
	}
	rows, _, err := client.QueryContextWithArgs(ctx, bound.SQL, bound.Args)
	if err != nil {
		t.Fatalf("QueryContextWithArgs 返回错误: %v", err)
	}
	if len(rows) != 0 {
		t.Fatalf("同一时刻不可能同时大于等于和小于: %#v", rows)
	}
}

func TestSQLiteContractDriversImplementArgsInterfaces(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	var _ QueryArgsContexter = client
	var _ ExecArgsContexter = client
	if !reflect.TypeOf(client).Implements(reflect.TypeOf((*QueryArgsContexter)(nil)).Elem()) {
		t.Fatal("SQLiteDB 应实现 QueryArgsContexter")
	}
}

func TestSQLiteQuotedTemplateParamBindsRenderedString(t *testing.T) {
	client := newArgsTestSQLiteDB(t)
	ctx := context.Background()

	// 用户场景（Navicat 风格）：'{param} 固定后缀' → 整段绑定渲染后的字符串
	bound, err := sqlparam.Bind(
		"SELECT id, name FROM bind_items WHERE created_at >= '{u_startDate} 00:00:00' ORDER BY id",
		"sqlite",
		map[string]sqlparam.TypedValue{
			"u_startDate": {Type: sqlparam.TypeDatetime, Value: "2026-08-02"},
		},
	)
	if err != nil {
		t.Fatalf("Bind 返回错误: %v", err)
	}
	t.Logf("DIAG sql=%q args=%#v", bound.SQL, bound.Args)
	if bound.SQL != "SELECT id, name FROM bind_items WHERE created_at >= ? ORDER BY id" {
		t.Fatalf("模板应整段替换为占位符: %q", bound.SQL)
	}
	if !reflect.DeepEqual(bound.Args, []any{"2026-08-02 00:00:00"}) {
		t.Fatalf("渲染值异常: %#v", bound.Args)
	}
	rows, _, err := client.QueryContextWithArgs(ctx, bound.SQL, bound.Args)
	if err != nil {
		t.Fatalf("QueryContextWithArgs 返回错误: %v", err)
	}
	if len(rows) != 2 || rows[0]["id"] != int64(2) || rows[1]["id"] != int64(3) {
		t.Fatalf("模板参数查询结果异常: %#v", rows)
	}
}
