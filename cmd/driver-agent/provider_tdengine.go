//go:build superlink_tdengine_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "tdengine"
	agentDatabaseFactory = func() db.Database {
		return &db.TDengineDB{}
	}
}
