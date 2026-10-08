//go:build superlink_dameng_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "dameng"
	agentDatabaseFactory = func() db.Database {
		return &db.DamengDB{}
	}
}
