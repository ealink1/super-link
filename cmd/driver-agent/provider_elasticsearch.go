//go:build superlink_elasticsearch_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "elasticsearch"
	agentDatabaseFactory = func() db.Database {
		return &db.ElasticsearchDB{}
	}
}
