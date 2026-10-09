package runtime

import (
	"database/sql"
	"errors"
)

// Driver agents transport Go's sql.ErrConnDone as an exact error string. Restore
// only this sentinel: database/sql returns it before dispatching any statement.
// Do not treat generic network/SQL errors as proof that a write did not execute.
func connectionExecutionError(err error) error {
	if err != nil && (errors.Is(err, sql.ErrConnDone) || err.Error() == sql.ErrConnDone.Error()) {
		return sql.ErrConnDone
	}
	return err
}
