//go:build superlink_mariadb_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "mariadb"
	agentDatabaseFactory = func() db.Database {
		return &db.MariaDB{}
	}
}
