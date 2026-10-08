//go:build superlink_sqlite_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "sqlite"
	agentDatabaseFactory = func() db.Database {
		return &db.SQLiteDB{}
	}
}
