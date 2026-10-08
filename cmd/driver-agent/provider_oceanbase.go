//go:build superlink_oceanbase_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "oceanbase"
	agentDatabaseFactory = func() db.Database {
		return &db.OceanBaseDB{}
	}
}
