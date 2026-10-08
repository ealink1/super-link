//go:build superlink_duckdb_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "duckdb"
	agentDatabaseFactory = func() db.Database {
		return &db.DuckDB{}
	}
}
