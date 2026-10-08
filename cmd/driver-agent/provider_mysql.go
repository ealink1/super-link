//go:build superlink_mysql_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "mysql"
	agentDatabaseFactory = func() db.Database {
		return &db.MySQLDB{}
	}
}
