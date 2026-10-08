//go:build superlink_opengauss_driver

package main

import "github.com/ealink1/super-link/internal/upstream/db"

func init() {
	agentDriverType = "opengauss"
	agentDatabaseFactory = func() db.Database {
		return &db.OpenGaussDB{}
	}
}
