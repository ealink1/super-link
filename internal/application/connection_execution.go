package application

import (
	"context"
	"database/sql"
	"errors"

	"github.com/ealink1/super-link/internal/domain"
)

// Called while holding the session gate and after policy/confirmation checks.
func (e *Engine) executeOnSession(ctx context.Context, p domain.Profile, s *session, request domain.Execution) ([]domain.Result, error) {
	results, err := s.client.Execute(ctx, request)
	descriptor, _ := domain.Resolve(p.Config.Type)
	if !errors.Is(err, sql.ErrConnDone) || descriptor.Family != domain.SQL || p.SQLDialect() == "sqlite" || p.SQLDialect() == "duckdb" || ctx.Err() != nil {
		return results, err
	}
	// ErrConnDone guarantees that database/sql never sent the statement. A single
	// reconnect is safe here; generic transport errors remain non-retryable.
	_ = s.client.Close()
	s.client = nil
	s.setConnectionStatus(ConnectionConnecting)
	if err = e.checkRevision(ctx, p.ID, p.Revision); err != nil {
		s.setConnectionStatus(ConnectionFailed)
		return nil, err
	}
	s.client, err = e.Factory(ctx, p)
	if err != nil {
		s.setConnectionStatus(ConnectionFailed)
		return nil, err
	}
	s.setConnectionStatus(ConnectionConnected)
	if err = e.checkRevision(ctx, p.ID, p.Revision); err != nil {
		return nil, err
	}
	return s.client.Execute(ctx, request)
}
