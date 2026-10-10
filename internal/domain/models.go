package domain

import (
	"errors"
	"time"

	"github.com/ealink1/super-link/internal/upstream/connection"
)

var (
	ErrNotFound       = errors.New("not found")
	ErrConflict       = errors.New("connection changed; reload before saving")
	ErrClosed         = errors.New("application is closing")
	ErrReadOnly       = errors.New("this connection is read-only")
	ErrUnknownOutcome = errors.New("server write outcome is unknown; inspect the server before retrying")
	ErrBusy           = errors.New("another operation is using this session")
)

type Profile struct {
	CreatedAt       time.Time                   `json:"createdAt,omitempty"`
	ID              string                      `json:"id"`
	Name            string                      `json:"name"`
	Group           string                      `json:"group"`
	Environment     string                      `json:"environment"`
	IconType        string                      `json:"iconType,omitempty"`
	IconColor       string                      `json:"iconColor,omitempty"`
	DatabaseAllow   []string                    `json:"databaseAllow,omitempty"`
	DatabaseInclude []string                    `json:"databaseInclude,omitempty"`
	DatabaseExclude []string                    `json:"databaseExclude,omitempty"`
	ReadOnly        bool                        `json:"readOnly"`
	Revision        int64                       `json:"revision"`
	SecretRef       string                      `json:"-"`
	Config          connection.ConnectionConfig `json:"config"`
	Scope           string                      `json:"-"` // Transient selection, separate from Oracle's service/SID.
}

type Column struct {
	ID   string
	Name string
}
type Result struct {
	Columns      []Column
	Rows         [][]any
	Messages     []string
	RowsAffected int64
	Truncated    bool
	Duration     time.Duration
}

type Object struct {
	Name   string
	Kind   string
	Scope  string
	Schema string
}

type Execution struct {
	Revision     int64
	Parameters   map[string]Parameter
	Args         []any  // Prepared by the application service; never interpolated into SQL.
	Action       string // Empty means interactive script; structure uses its own protection.
	Schema       string
	Text         string
	Scope        string
	MaxRows      int // Zero uses the application preview budget.
	Write        bool
	Confirmation string
}

type Parameter struct {
	Type  string
	Value any
}

type ConfirmationRequired struct{ Fingerprint string }

func (e *ConfirmationRequired) Error() string {
	return "confirm the operation and its target before execution"
}

type Draft struct {
	ID        string    `json:"id"`
	ProfileID string    `json:"profileId"`
	Scope     string    `json:"scope"`
	Schema    string    `json:"schema,omitempty"`
	Title     string    `json:"title"`
	Text      string    `json:"text"`
	UpdatedAt time.Time `json:"updatedAt"`
	Closed    bool      `json:"closed,omitempty"`
}

type History struct {
	ID        int64
	ProfileID string
	Text      string
	Success   bool
	Rows      int64
	Duration  time.Duration
	CreatedAt time.Time
}
