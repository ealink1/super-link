package runtime

import (
	"database/sql"
	"errors"
	"testing"
)

func TestConnectionExecutionErrorRestoresOnlyClosedBeforeDispatch(t *testing.T) {
	if !errors.Is(connectionExecutionError(errors.New(sql.ErrConnDone.Error())), sql.ErrConnDone) {
		t.Fatal("agent sentinel not restored")
	}
	for _, text := range []string{"connection reset", "driver: bad connection", "server: " + sql.ErrConnDone.Error()} {
		err := errors.New(text)
		if connectionExecutionError(err) != err {
			t.Fatal("uncertain error classified as unexecuted")
		}
	}
}
