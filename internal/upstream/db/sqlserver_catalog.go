package db

import (
	"fmt"
	"strconv"
	"strings"
)

const (
	// Azure SQL Database / Synapse serverless / Fabric reject or hang on
	// three-part catalog names such as [db].sys.tables even for the current
	// database. SuperLink already opens a DSN with database=dbName, so two-part
	// sys.* names are the portable catalog form.
	sqlServerEngineEditionAzureSQLDatabase       = 5
	sqlServerEngineEditionAzureSynapseAnalytics  = 6
	sqlServerEngineEditionAzureSQLEdge           = 9
	sqlServerEngineEditionAzureSynapseServerless = 11
	sqlServerEngineEditionMicrosoftFabric        = 12
)

func sqlServerUsesCurrentDatabaseCatalogOnly(edition int) bool {
	switch edition {
	case sqlServerEngineEditionAzureSQLDatabase,
		sqlServerEngineEditionAzureSynapseAnalytics,
		sqlServerEngineEditionAzureSQLEdge,
		sqlServerEngineEditionAzureSynapseServerless,
		sqlServerEngineEditionMicrosoftFabric:
		return true
	default:
		return false
	}
}

func sqlServerAccessibleDatabasesQuery() string {
	return "SELECT name FROM sys.databases WHERE state = 0 AND HAS_DBACCESS(name) = 1 ORDER BY name"
}

func sqlServerCurrentDatabaseQuery() string {
	return "SELECT DB_NAME() AS name"
}

func sqlServerEngineEditionQuery() string {
	return "SELECT CAST(SERVERPROPERTY('EngineEdition') AS int) AS edition"
}

func sqlServerListTablesQuery() string {
	return `
SELECT s.name AS schema_name, t.name AS table_name
FROM sys.tables t
JOIN sys.schemas s ON t.schema_id = s.schema_id
WHERE t.type = 'U'
ORDER BY s.name, t.name`
}

func sqlServerIntFromValue(value interface{}) (int, bool) {
	switch typed := value.(type) {
	case int:
		return typed, true
	case int8:
		return int(typed), true
	case int16:
		return int(typed), true
	case int32:
		return int(typed), true
	case int64:
		return int(typed), true
	case uint:
		return int(typed), true
	case uint8:
		return int(typed), true
	case uint16:
		return int(typed), true
	case uint32:
		return int(typed), true
	case uint64:
		return int(typed), true
	case float32:
		return int(typed), true
	case float64:
		return int(typed), true
	case []byte:
		parsed, err := strconv.Atoi(strings.TrimSpace(string(typed)))
		return parsed, err == nil
	case string:
		parsed, err := strconv.Atoi(strings.TrimSpace(typed))
		return parsed, err == nil
	default:
		return 0, false
	}
}

func collectSQLServerNameColumn(data []map[string]interface{}) []string {
	var names []string
	seen := make(map[string]struct{}, len(data))
	for _, row := range data {
		raw, ok := row["name"]
		if !ok || raw == nil {
			continue
		}
		name := strings.TrimSpace(fmt.Sprintf("%v", raw))
		if name == "" {
			continue
		}
		key := strings.ToLower(name)
		if _, exists := seen[key]; exists {
			continue
		}
		seen[key] = struct{}{}
		names = append(names, name)
	}
	return names
}
