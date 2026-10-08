//go:build superlink_cache_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "cache"
	agentDatabaseFactory = func() db.Database {
		return &db.CacheDB{}
	}
}
