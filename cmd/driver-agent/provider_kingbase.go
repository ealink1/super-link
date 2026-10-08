//go:build superlink_kingbase_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "kingbase"
	agentDatabaseFactory = func() db.Database {
		return &db.KingbaseDB{}
	}
}
