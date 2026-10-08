//go:build superlink_trino_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "trino"
	agentDatabaseFactory = func() db.Database {
		return &db.TrinoDB{}
	}
}
