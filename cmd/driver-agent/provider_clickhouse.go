//go:build superlink_clickhouse_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "clickhouse"
	agentDatabaseFactory = func() db.Database {
		return &db.ClickHouseDB{}
	}
}
