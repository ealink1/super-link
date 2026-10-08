//go:build superlink_diros_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "diros"
	agentDatabaseFactory = func() db.Database {
		return &db.DirosDB{}
	}
}
