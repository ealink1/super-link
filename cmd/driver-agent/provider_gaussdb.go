//go:build superlink_gaussdb_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "gaussdb"
	agentDatabaseFactory = func() db.Database {
		return &db.GaussDB{}
	}
}
