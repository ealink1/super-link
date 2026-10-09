package ui

import (
	"github.com/ealink1/super-link/internal/application"
	"strings"
	"testing"
	"time"
)

func TestSQLFileDetailedProgress(t *testing.T) {
	text, fraction := sqlFileProgressText("/tmp/import.sql", application.SQLFileProgress{Phase: "核对文件", Checked: 12, Bytes: 1 << 20, TotalBytes: 2 << 20}, 2*time.Second, time.Time{})
	if fraction != 0.5 || !strings.Contains(text, "已核对：12") || !strings.Contains(text, "0.5 MiB/s") {
		t.Fatal(text, fraction)
	}
	text, fraction = sqlFileProgressText("/tmp/import.sql", application.SQLFileProgress{Phase: "执行 SQL", Completed: 3, Current: 4, Total: 10}, 3*time.Second, time.Now().Add(-time.Second))
	if fraction != 0.3 || !strings.Contains(text, "正在执行第 4 条") || !strings.Contains(text, "已执行：3 / 10") {
		t.Fatal(text, fraction)
	}
}
