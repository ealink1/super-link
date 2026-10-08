//go:build superlink_highgo_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "highgo"
	agentDatabaseFactory = func() db.Database {
		return &db.HighGoDB{}
	}
}
