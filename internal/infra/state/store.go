// Package state persists application metadata. Credentials belong to the vault.
package state

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	_ "modernc.org/sqlite"
)

type Store struct{ db *sql.DB }

func Open(path string) (*Store, error) {
	if !filepath.IsAbs(path) {
		return nil, errors.New("state path must be absolute")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	if err = f.Close(); err != nil {
		return nil, err
	}
	if err = os.Chmod(path, 0600); err != nil {
		return nil, err
	}
	database, err := sql.Open("sqlite", databaseURI(filepath.ToSlash(path))+"?_pragma=busy_timeout(5000)&_pragma=foreign_keys(1)")
	if err != nil {
		return nil, err
	}
	database.SetMaxOpenConns(1)
	s := &Store{db: database}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err = s.migrate(ctx); err != nil {
		_ = database.Close()
		return nil, err
	}
	return s, nil
}

func databaseURI(path string) string {
	// A drive letter must be a URI path, never an authority (file://C:/...).
	if len(path) >= 2 && path[1] == ':' {
		path = "/" + path
	}
	return (&url.URL{Scheme: "file", Path: path}).String()
}

func (s *Store) migrate(ctx context.Context) error {
	if _, err := s.db.ExecContext(ctx, "PRAGMA journal_mode=WAL"); err != nil {
		return err
	}
	var version int
	if err := s.db.QueryRowContext(ctx, "PRAGMA user_version").Scan(&version); err != nil {
		return err
	}
	if version > 2 {
		return fmt.Errorf("state schema %d is newer than this application", version)
	}
	if version == 2 {
		return nil
	}
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if version == 0 {
		_, err = tx.ExecContext(ctx, `
 CREATE TABLE profiles(id TEXT PRIMARY KEY, name TEXT NOT NULL, metadata TEXT NOT NULL, secret_ref TEXT NOT NULL, revision INTEGER NOT NULL);
 CREATE TABLE drafts(id TEXT PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, metadata TEXT NOT NULL);
 CREATE TABLE history(id INTEGER PRIMARY KEY AUTOINCREMENT, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, text TEXT NOT NULL, success INTEGER NOT NULL, rows_count INTEGER NOT NULL, duration_ns INTEGER NOT NULL, created_at INTEGER NOT NULL);
 CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
 PRAGMA user_version=1;`)
		if err != nil {
			return err
		}
	}
	_, err = tx.ExecContext(ctx, `CREATE TABLE saved_queries(id TEXT PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, title TEXT NOT NULL, scope TEXT NOT NULL, schema_name TEXT NOT NULL, text TEXT NOT NULL, revision INTEGER NOT NULL, updated_at INTEGER NOT NULL); PRAGMA user_version=2;`)
	if err != nil {
		return err
	}
	return tx.Commit()
}

func (s *Store) Close() error { return s.db.Close() }

func (s *Store) Profiles(ctx context.Context) ([]domain.Profile, error) {
	rows, err := s.db.QueryContext(ctx, "SELECT metadata,secret_ref,revision FROM profiles ORDER BY name COLLATE NOCASE,id")
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := []domain.Profile{}
	for rows.Next() {
		var raw, ref string
		var revision int64
		if err = rows.Scan(&raw, &ref, &revision); err != nil {
			return nil, err
		}
		var p domain.Profile
		if err = json.Unmarshal([]byte(raw), &p); err != nil {
			return nil, err
		}
		p.SecretRef, p.Revision = ref, revision
		result = append(result, p)
	}
	return result, rows.Err()
}

func (s *Store) Profile(ctx context.Context, id string) (domain.Profile, error) {
	var p domain.Profile
	var raw string
	err := s.db.QueryRowContext(ctx, "SELECT metadata,secret_ref,revision FROM profiles WHERE id=?", id).Scan(&raw, &p.SecretRef, &p.Revision)
	if errors.Is(err, sql.ErrNoRows) {
		return p, domain.ErrNotFound
	}
	if err != nil {
		return p, err
	}
	ref, revision := p.SecretRef, p.Revision
	err = json.Unmarshal([]byte(raw), &p)
	p.SecretRef, p.Revision = ref, revision
	return p, err
}

// SaveProfile compares revisions so two editors cannot overwrite each other.
func (s *Store) SaveProfile(ctx context.Context, p domain.Profile) (domain.Profile, error) {
	raw, err := json.Marshal(p)
	if err != nil {
		return p, err
	}
	var res sql.Result
	if p.Revision == 0 {
		res, err = s.db.ExecContext(ctx, "INSERT INTO profiles(id,name,metadata,secret_ref,revision) VALUES(?,?,?,?,1) ON CONFLICT(id) DO NOTHING", p.ID, p.Name, string(raw), p.SecretRef)
	} else {
		res, err = s.db.ExecContext(ctx, "UPDATE profiles SET name=?,metadata=?,secret_ref=?,revision=revision+1 WHERE id=? AND revision=?", p.Name, string(raw), p.SecretRef, p.ID, p.Revision)
	}
	if err != nil {
		return p, err
	}
	count, err := res.RowsAffected()
	if err != nil {
		return p, err
	}
	if count != 1 {
		return p, domain.ErrConflict
	}
	p.Revision++
	return p, nil
}

func (s *Store) DeleteProfile(ctx context.Context, id string, revision int64) error {
	res, err := s.db.ExecContext(ctx, "DELETE FROM profiles WHERE id=? AND revision=?", id, revision)
	if err != nil {
		return err
	}
	count, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if count != 1 {
		return domain.ErrConflict
	}
	return nil
}

func (s *Store) SaveDraft(ctx context.Context, d domain.Draft) error {
	if len(d.Text) > 1<<20 {
		return errors.New("draft exceeds 1 MiB")
	}
	d.UpdatedAt = time.Now().UTC()
	raw, err := json.Marshal(d)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(ctx, "INSERT INTO drafts(id,profile_id,metadata) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata,profile_id=excluded.profile_id", d.ID, d.ProfileID, string(raw))
	return err
}

func (s *Store) Drafts(ctx context.Context) ([]domain.Draft, error) {
	rows, err := s.db.QueryContext(ctx, "SELECT metadata FROM drafts ORDER BY rowid")
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := []domain.Draft{}
	for rows.Next() {
		var raw string
		var d domain.Draft
		if err = rows.Scan(&raw); err != nil {
			return nil, err
		}
		if err = json.Unmarshal([]byte(raw), &d); err != nil {
			return nil, err
		}
		result = append(result, d)
	}
	return result, rows.Err()
}

func (s *Store) DeleteDraft(ctx context.Context, id string) error {
	_, err := s.db.ExecContext(ctx, "DELETE FROM drafts WHERE id=?", id)
	return err
}

// History stores redacted query text supplied by the application; never results.
func (s *Store) AddHistory(ctx context.Context, h domain.History) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	_, err = tx.ExecContext(ctx, "INSERT INTO history(profile_id,text,success,rows_count,duration_ns,created_at) VALUES(?,?,?,?,?,?)", h.ProfileID, h.Text, h.Success, h.Rows, int64(h.Duration), time.Now().UnixMilli())
	if err != nil {
		return err
	}
	_, err = tx.ExecContext(ctx, "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT 1000)")
	if err != nil {
		return err
	}
	return tx.Commit()
}

func (s *Store) History(ctx context.Context, id string) ([]domain.History, error) {
	rows, err := s.db.QueryContext(ctx, "SELECT id,profile_id,text,success,rows_count,duration_ns,created_at FROM history WHERE profile_id=? ORDER BY id DESC LIMIT 100", id)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := []domain.History{}
	for rows.Next() {
		var h domain.History
		var ns, ms int64
		if err = rows.Scan(&h.ID, &h.ProfileID, &h.Text, &h.Success, &h.Rows, &ns, &ms); err != nil {
			return nil, err
		}
		h.Duration = time.Duration(ns)
		h.CreatedAt = time.UnixMilli(ms)
		result = append(result, h)
	}
	return result, rows.Err()
}

func (s *Store) Setting(ctx context.Context, key string) (string, error) {
	var value string
	err := s.db.QueryRowContext(ctx, "SELECT value FROM settings WHERE key=?", key).Scan(&value)
	if errors.Is(err, sql.ErrNoRows) {
		return "", nil
	}
	return value, err
}
func (s *Store) SetSetting(ctx context.Context, key, value string) error {
	if strings.TrimSpace(key) == "" {
		return errors.New("empty setting key")
	}
	_, err := s.db.ExecContext(ctx, "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", key, value)
	return err
}
