//go:build superlink_sphinx_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "sphinx"
	agentDatabaseFactory = func() db.Database {
		return &db.SphinxDB{}
	}
}
