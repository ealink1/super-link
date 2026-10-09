package application

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"github.com/ealink1/super-link/internal/upstream/sqlaudit"
	"github.com/google/uuid"
)

type session struct {
	gate              chan struct{}
	client            adapter.Client
	revision          int64
	connectionStatus  atomic.Uint32
	connectionChanges chan<- struct{}
}
type permission struct {
	fingerprint string
	expires     time.Time
}
type Engine struct {
	Profiles          *Profiles
	Factory           adapter.Factory
	mu                sync.Mutex
	sessions          map[string]*session
	permissions       map[string]permission
	closed            bool
	connectionChanges chan struct{}
}

func NewEngine(profiles *Profiles) *Engine {
	// One pending notification is sufficient: the UI reads the latest snapshot.
	return &Engine{Profiles: profiles, Factory: adapter.Open, sessions: map[string]*session{}, permissions: map[string]permission{}, connectionChanges: make(chan struct{}, 1)}
}

func (e *Engine) acquire(ctx context.Context, id, scope string) (domain.Profile, *session, func(), error) {
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return p, nil, nil, err
	}
	sessionScope := scope
	descriptor, _ := domain.Resolve(p.Config.Type)
	if descriptor.Family != domain.SQL && descriptor.Family != domain.Document || descriptor.Key == "sqlite" || descriptor.Key == "duckdb" {
		sessionScope = ""
	}
	key := id + "\x00" + sessionScope
	e.mu.Lock()
	if e.closed {
		e.mu.Unlock()
		return p, nil, nil, domain.ErrClosed
	}
	s := e.sessions[key]
	if s == nil {
		if len(e.sessions) >= 64 {
			e.mu.Unlock()
			return p, nil, nil, errors.New("session limit reached; disconnect unused connections")
		}
		s = &session{gate: make(chan struct{}, 1), connectionChanges: e.connectionChanges}
		e.sessions[key] = s
	}
	e.mu.Unlock()
	select {
	case s.gate <- struct{}{}:
	case <-ctx.Done():
		return p, nil, nil, ctx.Err()
	}
	release := func() { <-s.gate }
	e.mu.Lock()
	closed := e.closed
	e.mu.Unlock()
	if closed {
		release()
		return p, nil, nil, domain.ErrClosed
	}
	if s.client != nil && s.revision != p.Revision {
		_ = s.client.Close()
		s.client = nil
		s.setConnectionStatus(ConnectionDisconnected)
	}
	if s.client == nil {
		s.setConnectionStatus(ConnectionConnecting)
		p, err = adapter.ConfigForScope(p, scope)
		if err != nil {
			s.setConnectionStatus(ConnectionFailed)
			release()
			return p, nil, nil, err
		}
		s.client, err = e.Factory(ctx, p)
		if err != nil {
			s.setConnectionStatus(ConnectionFailed)
			release()
			return p, nil, nil, err
		}
		s.revision = p.Revision
		s.setConnectionStatus(ConnectionConnected)
	}
	return p, s, release, nil
}

func (e *Engine) Test(ctx context.Context, p domain.Profile) error {
	p.Config.Timeout = min(max(p.Config.Timeout, 15), 120)
	ctx, cancel := context.WithTimeout(ctx, time.Duration(p.Config.Timeout)*time.Second)
	defer cancel()
	client, err := e.Factory(ctx, p)
	if err != nil {
		return err
	}
	return client.Close()
}
func (e *Engine) Scopes(ctx context.Context, id string) ([]string, error) {
	p, s, release, err := e.acquire(ctx, id, "")
	if err != nil {
		return nil, err
	}
	defer release()
	names, err := s.client.Scopes(ctx)
	return p.VisibleDatabases(names), err
}
func (e *Engine) Objects(ctx context.Context, id, scope string) ([]domain.Object, error) {
	_, s, release, err := e.acquire(ctx, id, scope)
	if err != nil {
		return nil, err
	}
	defer release()
	return s.client.Objects(ctx, scope)
}
func (e *Engine) Schema(ctx context.Context, id, scope, name string) (string, error) {
	_, s, release, err := e.acquire(ctx, id, scope)
	if err != nil {
		return "", err
	}
	defer release()
	return s.client.Schema(ctx, scope, name)
}

func (e *Engine) Execute(ctx context.Context, id string, request domain.Execution) ([]domain.Result, error) {
	// Check policy before connecting; forbidden input cannot cause network side effects.
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return nil, err
	}
	if request.Revision != 0 && p.Revision != request.Revision {
		return nil, domain.ErrConflict
	}
	request, err = prepareExecution(p, request)
	if err != nil {
		return nil, err
	}
	descriptor, err := domain.Resolve(p.Config.Type)
	if err != nil {
		return nil, err
	}
	if descriptor.Family == domain.SQL {
		descriptor.Key = p.SQLDialect()
	}
	write, err := Classify(descriptor, request)
	if err != nil {
		return nil, err
	}
	if write && !request.Write {
		return nil, errors.New("this command can change state; enable the explicit write action")
	}
	if (write || request.Write) && p.ReadOnly {
		return nil, domain.ErrReadOnly
	}
	if request.Action != "" && request.Action != "structure" {
		return nil, errors.New("unsupported execution action")
	}
	if err = validateStructureScript(request, p.SQLDialect()); err != nil {
		return nil, err
	}
	if write || request.Write {
		if request.Action == "structure" && p.Config.Protection.RestrictStructureEdit || request.Action == "" && p.Config.Protection.RestrictScriptExecution {
			return nil, domain.ErrReadOnly
		}
	}
	if request.Write {
		if err = e.authorize(p, request); err != nil {
			return nil, err
		}
	}
	if p.Config.QueryTimeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, time.Duration(p.Config.QueryTimeout)*time.Second)
		defer cancel()
	}
	current, s, release, err := e.acquire(ctx, id, request.Scope)
	if err != nil {
		return nil, err
	}
	defer release()
	if current.Revision != p.Revision {
		return nil, domain.ErrConflict
	}
	start := time.Now()
	results, runError := e.executeOnSession(ctx, current, s, request)
	elapsed := time.Since(start)
	for i := range results {
		results[i].Duration = elapsed
	}
	// Broken/cancelled transports are discarded; writes are never retried.
	if runError != nil {
		if s.client != nil {
			_ = s.client.Close()
		}
		s.client = nil
		s.setConnectionStatus(ConnectionFailed)
	}
	historyCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	rows := int64(0)
	for _, result := range results {
		rows += int64(len(result.Rows)) + result.RowsAffected
	}
	if err = e.Profiles.Store.AddHistory(historyCtx, domain.History{ProfileID: id, Text: sqlaudit.RedactQuery(descriptor.Key, request.Text), Success: runError == nil, Rows: rows, Duration: elapsed}); err != nil && runError == nil {
		for i := range results {
			results[i].Messages = append(results[i].Messages, "Execution completed, but local history could not be saved.")
		}
	}
	if runError != nil && request.Write {
		return results, fmt.Errorf("write failed or was interrupted; inspect the server before retrying (outcome may be unknown): %w", runError)
	}
	return results, runError
}

func (e *Engine) authorize(p domain.Profile, request domain.Execution) error {
	raw, _ := json.Marshal(struct {
		ID                          string
		Revision                    int64
		Text, Scope, Schema, Action string
		Args                        []any
	}{p.ID, p.Revision, strings.TrimSpace(request.Text), request.Scope, request.Schema, request.Action, request.Args})
	hash := sha256.Sum256(raw)
	fingerprint := hex.EncodeToString(hash[:])
	e.mu.Lock()
	defer e.mu.Unlock()
	now := time.Now()
	for token, grant := range e.permissions {
		if now.After(grant.expires) {
			delete(e.permissions, token)
		}
	}
	if grant, ok := e.permissions[request.Confirmation]; ok {
		delete(e.permissions, request.Confirmation)
		if grant.fingerprint == fingerprint && now.Before(grant.expires) {
			return nil
		}
	}
	if len(e.permissions) >= 100 {
		return errors.New("too many pending confirmations")
	}
	token := uuid.NewString()
	e.permissions[token] = permission{fingerprint: fingerprint, expires: now.Add(time.Minute)}
	return &domain.ConfirmationRequired{Fingerprint: token}
}

func (e *Engine) Disconnect(ctx context.Context, id string) error {
	e.mu.Lock()
	sessions := []*session{}
	for key, s := range e.sessions {
		if strings.HasPrefix(key, id+"\x00") {
			sessions = append(sessions, s)
		}
	}
	e.mu.Unlock()
	var errs []error
	for _, s := range sessions {
		select {
		case s.gate <- struct{}{}:
		case <-ctx.Done():
			return errors.Join(append(errs, ctx.Err())...)
		}
		if s.client != nil {
			errs = append(errs, s.client.Close())
			s.client = nil
		}
		s.setConnectionStatus(ConnectionDisconnected)
		<-s.gate
	}
	return errors.Join(errs...)
}
func (e *Engine) Close() error {
	e.mu.Lock()
	e.closed = true
	sessions := make([]*session, 0, len(e.sessions))
	for _, s := range e.sessions {
		sessions = append(sessions, s)
	}
	e.mu.Unlock()
	var errs []error
	for _, s := range sessions {
		s.gate <- struct{}{}
		if s.client != nil {
			errs = append(errs, s.client.Close())
			s.client = nil
		}
		s.setConnectionStatus(ConnectionDisconnected)
		<-s.gate
	}
	return errors.Join(errs...)
}
