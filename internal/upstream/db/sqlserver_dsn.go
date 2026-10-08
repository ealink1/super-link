//go:build superlink_full_drivers || superlink_sqlserver_driver

package db

import (
	"net"
	"net/url"
	"strconv"
	"strings"

	"github.com/ealink1/super-link/internal/upstream/connection"
)

func (s *SqlServerDB) getDSN(config connection.ConnectionConfig) string {
	return s.dsnForRemoteHost(config, config.Host)
}

// dsnForRemoteHost builds a DSN for the dial target while preserving the
// original remote server identity for Azure detection and certificate checks.
// SSH forwarding changes only the TCP endpoint; it must not change TLS
// hostname verification semantics.
func (s *SqlServerDB) dsnForRemoteHost(config connection.ConnectionConfig, remoteHost string) string {
	// sqlserver://user:password@host:port?database=dbname
	dbname := config.Database
	if dbname == "" {
		dbname = "master"
	}

	u := &url.URL{
		Scheme: "sqlserver",
		Host:   net.JoinHostPort(config.Host, strconv.Itoa(config.Port)),
	}
	u.User = url.UserPassword(config.User, config.Password)

	q := url.Values{}
	q.Set("database", dbname)
	q.Set("connection timeout", strconv.Itoa(getConnectTimeoutSeconds(config)))
	tlsConfig := config
	tlsConfig.Host = remoteHost
	encrypt, trustServerCertificate := resolveSQLServerTLSSettings(tlsConfig)
	q.Set("encrypt", encrypt)
	q.Set("trustservercertificate", trustServerCertificate)
	if strings.TrimSpace(config.SSLCAPath) != "" {
		q.Set("certificate", strings.TrimSpace(config.SSLCAPath))
	}
	// Leave user-supplied connection params untouched: go-mssqldb rejects the
	// mutually exclusive certificate combinations itself with an actionable
	// error. Silently dropping one of them would turn an explicit security
	// choice into an unexplained downgrade.
	mergeConnectionParamsFromConfigWithAllowlist(q, config, sqlServerConnectionParamNames, "sqlserver")
	// The driver defaults hostnameincertificate to the DSN host. Under SSH the
	// DSN host is the local forward address, so fill in the original remote
	// host unless the user configured either certificate identity explicitly.
	certificateHost := azureSQLHostNameInCertificate(remoteHost)
	if certificateHost == "" && !strings.EqualFold(strings.TrimSpace(config.Host), strings.TrimSpace(remoteHost)) {
		certificateHost = sqlServerHostNameInCertificate(remoteHost)
	}
	if certificateHost != "" && strings.TrimSpace(q.Get("hostnameincertificate")) == "" && strings.TrimSpace(q.Get("servercertificate")) == "" {
		q.Set("hostnameincertificate", certificateHost)
	}
	u.RawQuery = q.Encode()

	return u.String()
}
