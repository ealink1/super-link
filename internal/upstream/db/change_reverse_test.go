package db

import (
	"strings"
	"testing"
	"time"

	"github.com/ealink1/super-link/internal/upstream/connection"
)

// mysqlQuote 定义在 change_preview_test.go，同一包内复用。
// pgQuote 用 PostgreSQL / Oracle 风格的双引号引用标识符。
func pgQuote(s string) string { return `"` + s + `"` }

// oracleRowIDValueColumn 与 frontend/src/utils/rowLocator.ts 的
// ORACLE_ROWID_LOCATOR_COLUMN 保持一致：Oracle 行定位在 WHERE 里写 ROWID，
// 实际取值走查询时额外投影的别名列。
const oracleRowIDValueColumn = "__superlink_oracle_rowid__"

func TestGenerateChangeReverseUpdateRestoresPreviousValues(t *testing.T) {
	changes := connection.ChangeSet{
		Updates: []connection.UpdateRow{{
			Keys:           map[string]interface{}{"id": 7},
			Values:         map[string]interface{}{"name": "new", "status": "done"},
			PreviousValues: map[string]interface{}{"name": "old", "status": "open"},
		}},
	}

	result := GenerateChangeReverseWithDialect("t_order", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Updates) != 1 {
		t.Fatalf("expected 1 reverse update, got %d (%v)", len(result.Updates), result.Skipped)
	}
	// 列按字典序输出，保证确定性。
	want := "UPDATE `t_order` SET `name` = 'old', `status` = 'open' WHERE `id` = 7;"
	if result.Updates[0] != want {
		t.Fatalf("reverse update mismatch:\n got %s\nwant %s", result.Updates[0], want)
	}
	if !result.Complete {
		t.Fatalf("expected complete result, skipped=%v", result.Skipped)
	}
}

// 只还原本次真正改过的列：PreviousValues 里多带的列若被写回，会覆盖并发修改。
func TestGenerateChangeReverseUpdateOnlyTouchesChangedColumns(t *testing.T) {
	changes := connection.ChangeSet{
		Updates: []connection.UpdateRow{{
			Keys:   map[string]interface{}{"id": 1},
			Values: map[string]interface{}{"name": "new"},
			// untouched 用户没改，但它出现在 PreviousValues 里。
			PreviousValues: map[string]interface{}{"name": "old", "untouched": "keep-me"},
		}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Updates) != 1 {
		t.Fatalf("expected 1 reverse update, got %v", result.Skipped)
	}
	if strings.Contains(result.Updates[0], "untouched") {
		t.Fatalf("reverse update must not touch unchanged columns: %s", result.Updates[0])
	}
}

func TestGenerateChangeReverseDeleteReinsertsFullRow(t *testing.T) {
	changes := connection.ChangeSet{
		Deletes: []map[string]interface{}{{"id": 3, "name": "gone"}},
		PreviousDeletes: []map[string]interface{}{{
			"id": 3, "name": "gone", "amount": 12.5,
		}},
	}

	result := GenerateChangeReverseWithDialect("t_pay", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Inserts) != 1 {
		t.Fatalf("expected 1 reverse insert, got %d (%v)", len(result.Inserts), result.Skipped)
	}
	want := "INSERT INTO `t_pay` (`amount`, `id`, `name`) VALUES (12.5, 3, 'gone');"
	if result.Inserts[0] != want {
		t.Fatalf("reverse insert mismatch:\n got %s\nwant %s", result.Inserts[0], want)
	}
}

// 未携带 before-image 时必须跳过而不是猜测 —— 这是诚实性底线。
func TestGenerateChangeReverseSkipsRowsWithoutBeforeImage(t *testing.T) {
	changes := connection.ChangeSet{
		Deletes: []map[string]interface{}{{"id": 3}},
		Updates: []connection.UpdateRow{
			{Keys: map[string]interface{}{"id": 4}, Values: map[string]interface{}{"name": "x"}},
			{Keys: map[string]interface{}{"id": 5}, Values: map[string]interface{}{"name": "y"}},
		},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Updates) != 0 || len(result.Inserts) != 0 {
		t.Fatalf("expected no statements, got updates=%v inserts=%v", result.Updates, result.Inserts)
	}
	if result.Complete {
		t.Fatal("result must not be marked complete when rows were skipped")
	}
	if len(result.Skipped) != 3 {
		t.Fatalf("expected 3 skipped rows, got %d (%v)", len(result.Skipped), result.Skipped)
	}
	// 分组与下标必须能定位到原始行。
	groups := map[string]int{}
	for _, skipped := range result.Skipped {
		groups[skipped.Group]++
	}
	if groups["delete"] != 1 || groups["update"] != 2 {
		t.Fatalf("skipped groups mismatch: %v", groups)
	}
}

func TestGenerateChangeReverseInsertUsesLocatorColumns(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts:        []map[string]interface{}{{"id": 9, "name": "fresh"}},
		LocatorColumns: []connection.LocatorColumn{{Key: "id"}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 1 {
		t.Fatalf("expected 1 reverse delete, got %d (%v)", len(result.Deletes), result.Skipped)
	}
	want := "DELETE FROM `t` WHERE `id` = 9;"
	if result.Deletes[0] != want {
		t.Fatalf("reverse delete mismatch:\n got %s\nwant %s", result.Deletes[0], want)
	}
}

// 数据库生成的定位值（自增主键等）客户端拿不到，必须跳过 —— 用不完整的 WHERE
// 生成 DELETE 会误删其他行。
func TestGenerateChangeReverseInsertSkipsRowsWithMissingLocatorValue(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts:        []map[string]interface{}{{"name": "no-id"}},
		LocatorColumns: []connection.LocatorColumn{{Key: "id"}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 0 {
		t.Fatalf("must not emit delete without locator value, got %v", result.Deletes)
	}
	if result.Complete {
		t.Fatal("result must not be complete")
	}
	if len(result.Skipped) != 1 || result.Skipped[0].Group != "insert" {
		t.Fatalf("expected insert row to be skipped, got %v", result.Skipped)
	}
}

func TestGenerateChangeReverseInsertWithoutLocatorColumnsSkips(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts: []map[string]interface{}{{"id": 1}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 0 || result.Complete {
		t.Fatalf("expected skip when no locator columns, got %+v", result)
	}
	if result.Skipped[0].Reason != reverseReasonNoLocatorColumns {
		t.Fatalf("unexpected reason: %s", result.Skipped[0].Reason)
	}
}

// rowid 这类伪列策略：WHERE 用 Key，取值走 ValueColumn，且列名**不加引用**。
// Oracle 的 ROWID 是伪列，正向语句（oracle_impl.go:1613）也是裸名输出。
func TestGenerateChangeReverseInsertUsesUnquotedOracleRowID(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts: []map[string]interface{}{{
			oracleRowIDValueColumn: "AAAR8x",
			"name":                 "x",
		}},
		LocatorStrategy: "oracle-rowid",
		LocatorColumns: []connection.LocatorColumn{{
			Key: "ROWID", ValueColumn: oracleRowIDValueColumn,
		}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "oracle", pgQuote, pgQuote)

	if len(result.Deletes) != 1 {
		t.Fatalf("expected 1 reverse delete, got %d (%v)", len(result.Deletes), result.Skipped)
	}
	want := `DELETE FROM "t" WHERE ROWID = 'AAAR8x';`
	if result.Deletes[0] != want {
		t.Fatalf("reverse delete mismatch:\n got %s\nwant %s", result.Deletes[0], want)
	}
}

// DuckDB 的 rowid 同为伪列，正向语句（duckdb_impl.go:415）写作裸名 rowid。
func TestGenerateChangeReverseInsertUsesUnquotedDuckDBRowID(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts: []map[string]interface{}{{
			"__superlink_duckdb_rowid__": 17,
			"name":                       "x",
		}},
		LocatorStrategy: "duckdb-rowid",
		LocatorColumns: []connection.LocatorColumn{{
			Key: "rowid", ValueColumn: "__superlink_duckdb_rowid__",
		}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "duckdb", pgQuote, pgQuote)

	if len(result.Deletes) != 1 {
		t.Fatalf("expected 1 reverse delete, got %d (%v)", len(result.Deletes), result.Skipped)
	}
	want := `DELETE FROM "t" WHERE rowid = 17;`
	if result.Deletes[0] != want {
		t.Fatalf("reverse delete mismatch:\n got %s\nwant %s", result.Deletes[0], want)
	}
}

// 非 rowid 策略下同名"rowid"列是真实列，必须正常引用，不能误当伪列放行。
func TestGenerateChangeReverseQuotesRowIDForNonPseudoStrategies(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts:         []map[string]interface{}{{"rowid": "r-1"}},
		LocatorStrategy: "primary-key",
		LocatorColumns:  []connection.LocatorColumn{{Key: "rowid"}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 1 {
		t.Fatalf("expected 1 reverse delete, got %d (%v)", len(result.Deletes), result.Skipped)
	}
	want := "DELETE FROM `t` WHERE `rowid` = 'r-1';"
	if result.Deletes[0] != want {
		t.Fatalf("reverse delete mismatch:\n got %s\nwant %s", result.Deletes[0], want)
	}
}

// 空串定位值不可靠（可能真是空串，也可能是驱动未回传），必须跳过。
func TestGenerateChangeReverseInsertSkipsEmptyLocatorValue(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts:        []map[string]interface{}{{"id": ""}},
		LocatorColumns: []connection.LocatorColumn{{Key: "id"}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 0 || result.Complete {
		t.Fatalf("expected skip for empty locator value, got %+v", result)
	}
}

// 方言字面量格式化必须沿用 change_preview 的实现（MySQL 反斜杠转义 / 时间类型）。
func TestGenerateChangeReverseUsesDialectLiteralFormatting(t *testing.T) {
	moment := time.Date(2026, 9, 20, 13, 45, 30, 0, time.UTC)
	changes := connection.ChangeSet{
		Updates: []connection.UpdateRow{{
			Keys:           map[string]interface{}{"id": 1},
			Values:         map[string]interface{}{"note": "a\nb", "at": moment},
			PreviousValues: map[string]interface{}{"note": `back\slash`, "at": moment},
		}},
	}

	mysqlResult := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)
	if len(mysqlResult.Updates) != 1 {
		t.Fatalf("expected 1 update, got %v", mysqlResult.Skipped)
	}
	// MySQL 默认把反斜杠当转义符，必须写成双反斜杠。
	if !strings.Contains(mysqlResult.Updates[0], `back\\slash`) {
		t.Fatalf("mysql literal must escape backslash: %s", mysqlResult.Updates[0])
	}

	pgResult := GenerateChangeReverseWithDialect("t", changes, "postgres", pgQuote, pgQuote)
	if len(pgResult.Updates) != 1 {
		t.Fatalf("expected 1 pg update, got %v", pgResult.Skipped)
	}
	// PostgreSQL 的 standard_conforming_strings 下反斜杠不是转义符，保持原样。
	if !strings.Contains(pgResult.Updates[0], `back\slash`) {
		t.Fatalf("postgres literal must not double backslash: %s", pgResult.Updates[0])
	}
	if !strings.Contains(pgResult.Updates[0], "TIMESTAMP '2026-09-20 13:45:30'") {
		t.Fatalf("postgres temporal literal mismatch: %s", pgResult.Updates[0])
	}
}

// 三类变更混合时，输出分组要齐备且互不串位。
func TestGenerateChangeReverseHandlesMixedChangeSet(t *testing.T) {
	changes := connection.ChangeSet{
		Inserts: []map[string]interface{}{{"id": 10, "name": "added"}},
		Updates: []connection.UpdateRow{{
			Keys:           map[string]interface{}{"id": 11},
			Values:         map[string]interface{}{"name": "changed"},
			PreviousValues: map[string]interface{}{"name": "before"},
		}},
		Deletes:         []map[string]interface{}{{"id": 12}},
		PreviousDeletes: []map[string]interface{}{{"id": 12, "name": "removed"}},
		LocatorColumns:  []connection.LocatorColumn{{Key: "id"}},
	}

	result := GenerateChangeReverseWithDialect("t", changes, "mysql", mysqlQuote, mysqlQuote)

	if len(result.Deletes) != 1 || len(result.Updates) != 1 || len(result.Inserts) != 1 {
		t.Fatalf("mixed change set mismatch: deletes=%v updates=%v inserts=%v skipped=%v",
			result.Deletes, result.Updates, result.Inserts, result.Skipped)
	}
	if !result.Complete {
		t.Fatalf("expected complete, skipped=%v", result.Skipped)
	}
}

// 空变更集不产生语句，也不应被判为 complete —— 没有语句可回放。
func TestGenerateChangeReverseEmptyChangeSetIsNotComplete(t *testing.T) {
	result := GenerateChangeReverseWithDialect(
		"t", connection.ChangeSet{}, "mysql", mysqlQuote, mysqlQuote,
	)

	if result.Complete {
		t.Fatal("empty change set must not be marked complete")
	}
	if len(result.Skipped) != 0 {
		t.Fatalf("empty change set must not report skips: %v", result.Skipped)
	}
}
