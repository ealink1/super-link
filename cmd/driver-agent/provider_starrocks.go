//go:build superlink_starrocks_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "starrocks"
	agentDatabaseFactory = func() db.Database {
		return &db.StarRocksDB{}
	}
}
