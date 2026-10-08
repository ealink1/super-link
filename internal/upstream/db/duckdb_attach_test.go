//go:build superlink_full_drivers || superlink_duckdb_driver

package db

import (
	"context"
	"database/sql"
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// newDuckDBAttachTestInstance 打开一个 :memory: DuckDB 实例作为附加宿主。
func newDuckDBAttachTestInstance(t *testing.T) *DuckDB {
	t.Helper()
	conn, err := sql.Open("duckdb", ":memory:")
	if err != nil {
		t.Fatalf("open duckdb: %v", err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	return &DuckDB{conn: conn}
}

// newDuckDBAttachTargetFile 创建一个带数据的 .duckdb 文件作为附加目标。
func newDuckDBAttachTargetFile(t *testing.T, table string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "target.duckdb")
	fileDB, err := sql.Open("duckdb", path)
	if err != nil {
		t.Fatalf("open target: %v", err)
	}
	if _, err := fileDB.ExecContext(context.Background(),
		"CREATE TABLE "+table+" (id INTEGER); INSERT INTO "+table+" VALUES (42);"); err != nil {
		t.Fatalf("seed target: %v", err)
	}
	if err := fileDB.Close(); err != nil {
		t.Fatalf("close target: %v", err)
	}
	return path
}

func TestDuckDBAttachExternalDuckDBFileRoundTrip(t *testing.T) {
	host := newDuckDBAttachTestInstance(t)
	targetPath := newDuckDBAttachTargetFile(t, "orders")
	ctx := context.Background()

	spec := ExternalAttachSpec{
		Kind: ExternalAttachKindDuckDB, FilePath: targetPath, Alias: "ext_db", ReadOnly: true,
	}
	if err := host.AttachExternalDatabase(ctx, spec); err != nil {
		t.Fatalf("attach: %v", err)
	}

	var count int
	if err := host.conn.QueryRowContext(ctx, "SELECT count(*) FROM ext_db.orders").Scan(&count); err != nil {
		t.Fatalf("cross query: %v", err)
	}
	if count != 1 {
		t.Fatalf("count = %d, want 1", count)
	}

	// 只读附加：写入远端被拒绝
	if _, err := host.conn.ExecContext(ctx, "INSERT INTO ext_db.orders VALUES (7)"); err == nil {
		t.Fatalf("write into read-only attach should fail")
	}

	// 卸载后查询失败，再次卸载幂等
	if err := host.DetachExternalDatabase(ctx, "ext_db"); err != nil {
		t.Fatalf("detach: %v", err)
	}
	if err := host.DetachExternalDatabase(ctx, "ext_db"); !errors.Is(err, ErrExternalAttachNotAttached) {
		t.Fatalf("second detach = %v, want ErrExternalAttachNotAttached", err)
	}
	if _, err := host.conn.QueryContext(ctx, "SELECT * FROM ext_db.orders"); err == nil {
		t.Fatalf("query after detach should fail")
	}
}

func TestDuckDBAttachReplaceAndConflictSemantics(t *testing.T) {
	host := newDuckDBAttachTestInstance(t)
	fileA := newDuckDBAttachTargetFile(t, "marker_a")
	fileB := newDuckDBAttachTargetFile(t, "marker_b")
	// 宿主引擎持有目标文件句柄：TempDir 的 RemoveAll 清理注册晚于宿主关闭清理
	//（LIFO 先执行），必须先显式关闭宿主再删目录，否则 Windows 上文件占用报错。
	t.Cleanup(func() { _ = host.conn.Close() })
	ctx := context.Background()

	specA := ExternalAttachSpec{Kind: ExternalAttachKindDuckDB, FilePath: fileA, Alias: "ext_db"}
	if err := host.AttachExternalDatabase(ctx, specA); err != nil {
		t.Fatalf("attach A: %v", err)
	}
	// 同源重跑：替换重建成功
	if err := host.AttachExternalDatabase(ctx, specA); err != nil {
		t.Fatalf("idempotent re-attach: %v", err)
	}
	// 换文件同别名：冲突报错
	specB := ExternalAttachSpec{Kind: ExternalAttachKindDuckDB, FilePath: fileB, Alias: "ext_db"}
	err := host.AttachExternalDatabase(ctx, specB)
	if err == nil || !strings.Contains(err.Error(), "ext_db") {
		t.Fatalf("conflict error = %v", err)
	}
	// 冲突后原附加仍可用
	var count int
	if err := host.conn.QueryRowContext(ctx, "SELECT count(*) FROM ext_db.marker_a").Scan(&count); err != nil {
		t.Fatalf("original attach broken after conflict: %v", err)
	}
}

func TestDuckDBAttachAliasValidation(t *testing.T) {
	host := newDuckDBAttachTestInstance(t)
	for _, alias := range []string{"1bad", "with space", "semi;colon"} {
		err := host.AttachExternalDatabase(context.Background(), ExternalAttachSpec{
			Kind: ExternalAttachKindDuckDB, FilePath: "x.duckdb", Alias: alias,
		})
		if err == nil || !strings.Contains(err.Error(), alias) {
			t.Fatalf("alias %q: err = %v", alias, err)
		}
	}
}

// TestDuckDBAttachMySQLSecretSyntaxAndErrorSanitization 需要联网安装 mysql 扩展；
// 扩展不可用时跳过（与 duckdb_metadata_integration_test 的门控模式一致）。
// 通过“附加到不可达主机”验证：SECRET 语法被引擎接受、ATTACH 失败错误不含密码、
// 失败后无残留 SECRET。
func TestDuckDBAttachMySQLSecretSyntaxAndErrorSanitization(t *testing.T) {
	host := newDuckDBAttachTestInstance(t)
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	if err := host.ensureExtensionLoaded(ctx, "mysql"); err != nil {
		t.Skipf("mysql extension unavailable: %v", err)
	}

	const password = "s3cret-DO-NOT-LEAK"
	err := host.AttachExternalDatabase(ctx, ExternalAttachSpec{
		Kind: ExternalAttachKindMySQL, Host: "127.0.0.1", Port: 1,
		User: "superlink-test", Password: password, Database: "no_such_db",
		Alias: "mysql_ext", ReadOnly: true,
	})
	if err == nil {
		t.Skipf("127.0.0.1:1 unexpectedly accepted a mysql connection; cannot exercise failure path")
	}
	if strings.Contains(err.Error(), password) {
		t.Fatalf("attach error leaked password: %v", err)
	}
	var secretCount int
	if err := host.conn.QueryRowContext(ctx,
		"SELECT count(*) FROM duckdb_secrets() WHERE name = 'superlink_attach_mysql_ext'").Scan(&secretCount); err != nil {
		t.Fatalf("query secrets: %v", err)
	}
	if secretCount != 0 {
		t.Fatalf("failed attach left %d dangling secret(s)", secretCount)
	}
}

// TestDuckDBAttachMySQLUsesSecretDatabase 回归（上游审查 P0-1）：mysql 扩展把
// ATTACH 的 path 当主机 DSN，库名必须经 SECRET 的 DATABASE 传递。
//
// 实现注意：不读 duckdb_secrets() 的 database_value 列做断言——该视图暴露的列
// 随 DuckDB 版本漂移（旧版本只有 name/type/storage/scope），CI 会因列不存在而
// 挂。核心契约是"生成的 CREATE SECRET 语句确实写入 DATABASE"，改为直接断言
// 纯函数产出的 SQL；引擎侧只做 smoke 验证 SECRET 能成功创建。
func TestDuckDBAttachMySQLUsesSecretDatabase(t *testing.T) {
	spec := ExternalAttachSpec{
		Kind: ExternalAttachKindMySQL, Host: "127.0.0.1", Port: 3306,
		User: "superlink-test", Password: "pw", Database: "orders_db",
		Alias: "mysql_ext", SecretName: "superlink_attach_mysql_ext", ReadOnly: true,
	}

	// 1) 静态契约：ATTACH 路径必须为空，库名走 SECRET 的 DATABASE
	attachStmt := buildDuckDBAttachStatement(spec)
	if !strings.Contains(attachStmt, "ATTACH '' ") {
		t.Fatalf("attach path must be empty (mysql treats path as host DSN): %s", attachStmt)
	}
	secretStmt := buildCreateExternalSecretStatement(spec)
	if secretStmt == "" {
		t.Fatal("mysql must build a CREATE SECRET statement")
	}
	if !strings.Contains(secretStmt, "TYPE MYSQL") {
		t.Fatalf("secret statement missing TYPE MYSQL: %s", secretStmt)
	}
	if !strings.Contains(secretStmt, "DATABASE 'orders_db'") {
		t.Fatalf("secret statement missing DATABASE 'orders_db': %s", secretStmt)
	}

	// 2) 引擎 smoke：CREATE SECRET 能被当前 DuckDB 接受（不读版本敏感的 catalog 列）
	host := newDuckDBAttachTestInstance(t)
	ctx := context.Background()
	if err := host.ensureExtensionLoaded(ctx, "mysql"); err != nil {
		t.Skipf("mysql extension unavailable: %v", err)
	}
	if err := host.createExternalSecret(ctx, spec); err != nil {
		t.Fatalf("create secret: %v", err)
	}
	defer func() { host.dropExternalSecret(context.Background(), spec.SecretName) }()

	// 用 duckdb_secrets() 中跨版本稳定存在的 name 列确认 SECRET 已注册，
	// 避免依赖 database_value 等版本漂移列。
	var name string
	if err := host.conn.QueryRowContext(ctx,
		"SELECT name FROM duckdb_secrets() WHERE name = ?",
		spec.SecretName).Scan(&name); err != nil {
		t.Fatalf("read secret name: %v", err)
	}
	if name != spec.SecretName {
		t.Fatalf("registered secret name = %q, want %q", name, spec.SecretName)
	}
}

// TestDuckDBAttachRerunAfterNativeDetachIsIdempotent 回归（上游审查 P1-3/P1-4）：
// 原生 DETACH 不会移除 SECRET，重跑时残留 SECRET 不得让 CREATE 报 already exists；
// 失败路径的清理在取消派生 ctx 下也必须真实执行。
func TestDuckDBAttachRerunAfterNativeDetachIsIdempotent(t *testing.T) {
	host := newDuckDBAttachTestInstance(t)
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	if err := host.ensureExtensionLoaded(ctx, "mysql"); err != nil {
		t.Skipf("mysql extension unavailable: %v", err)
	}

	spec := ExternalAttachSpec{
		Kind: ExternalAttachKindMySQL, Host: "127.0.0.1", Port: 1,
		User: "superlink-test", Password: "pw", Database: "no_such_db",
		Alias: "mysql_ext", SecretName: externalSecretName("mysql_ext"), ReadOnly: true,
	}

	// 第一次附加：SECRET 建成后 ATTACH 拨号失败，失败路径清理 SECRET
	if err := host.AttachExternalDatabase(ctx, spec); err == nil {
		t.Skipf("127.0.0.1:1 unexpectedly accepted a mysql connection")
	}
	assertSecretCount(t, host, spec.SecretName, 0)

	// 模拟外部残留（原生 DETACH 泄漏的等价状态）：同名 SECRET 已存在
	if err := host.createExternalSecret(ctx, spec); err != nil {
		t.Fatalf("seed residual secret: %v", err)
	}
	assertSecretCount(t, host, spec.SecretName, 1)

	// 重跑：CREATE OR REPLACE 幂等覆盖，不得报 already exists；失败后再次清理
	err := host.AttachExternalDatabase(ctx, spec)
	if err == nil {
		t.Skipf("127.0.0.1:1 unexpectedly accepted a mysql connection")
	}
	if strings.Contains(err.Error(), "already exists") {
		t.Fatalf("rerun hit residual secret: %v", err)
	}
	assertSecretCount(t, host, spec.SecretName, 0)
}

func assertSecretCount(t *testing.T, host *DuckDB, secretName string, want int) {
	t.Helper()
	var count int
	if err := host.conn.QueryRowContext(context.Background(),
		"SELECT count(*) FROM duckdb_secrets() WHERE name = ?", secretName).Scan(&count); err != nil {
		t.Fatalf("read secret count: %v", err)
	}
	if count != want {
		t.Fatalf("secret %q count = %d, want %d", secretName, count, want)
	}
}

// TestSecretCleanupContextSurvivesParentCancel 钉住 P1-4 修复：ATTACH 失败最常见
// 于 ctx 超时，SECRET 清理必须使用不可取消派生的 ctx。
func TestSecretCleanupContextSurvivesParentCancel(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	cleanupCtx, cleanupCancel := secretCleanupContext(ctx)
	defer cleanupCancel()
	if err := cleanupCtx.Err(); err != nil {
		t.Fatalf("cleanup ctx canceled with parent: %v", err)
	}
}
