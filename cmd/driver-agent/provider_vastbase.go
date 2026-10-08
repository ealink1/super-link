//go:build superlink_vastbase_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "vastbase"
	agentDatabaseFactory = func() db.Database {
		return &db.VastbaseDB{}
	}
}
