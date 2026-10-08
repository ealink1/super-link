//go:build superlink_mongodb_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "mongodb"
	agentDatabaseFactory = func() db.Database {
		return &db.MongoDB{}
	}
}
