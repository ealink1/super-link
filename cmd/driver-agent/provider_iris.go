//go:build superlink_iris_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "iris"
	agentDatabaseFactory = func() db.Database {
		return &db.IrisDB{}
	}
}
