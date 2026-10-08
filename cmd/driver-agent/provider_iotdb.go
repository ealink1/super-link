//go:build superlink_iotdb_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "iotdb"
	agentDatabaseFactory = func() db.Database {
		return &db.IoTDBDB{}
	}
}
