package state

import (
	"context"
	"database/sql"
	"encoding/json"

	"github.com/ealink1/super-link/internal/domain"
)

func (s *Store) MoveProfilesToGroup(ctx context.Context, profiles []domain.Profile, group string) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if err := moveProfilesToGroup(ctx, tx, profiles, group); err != nil {
		return err
	}
	return tx.Commit()
}

func (s *Store) CreateConnectionGroup(ctx context.Context, settings map[string]string, profiles []domain.Profile, group string) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for key, value := range settings {
		if _, err := tx.ExecContext(ctx, "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", key, value); err != nil {
			return err
		}
	}
	if err := moveProfilesToGroup(ctx, tx, profiles, group); err != nil {
		return err
	}
	return tx.Commit()
}

func moveProfilesToGroup(ctx context.Context, tx *sql.Tx, profiles []domain.Profile, group string) error {
	for _, selected := range profiles {
		var raw string
		var revision int64
		if err := tx.QueryRowContext(ctx, "SELECT metadata,revision FROM profiles WHERE id=?", selected.ID).Scan(&raw, &revision); err != nil {
			return err
		}
		if revision != selected.Revision {
			return domain.ErrConflict
		}
		var current domain.Profile
		if err := json.Unmarshal([]byte(raw), &current); err != nil {
			return err
		}
		current.Group = group
		encoded, err := json.Marshal(current)
		if err != nil {
			return err
		}
		result, err := tx.ExecContext(ctx, "UPDATE profiles SET metadata=?,revision=revision+1 WHERE id=? AND revision=?", string(encoded), current.ID, revision)
		if err != nil {
			return err
		}
		count, err := result.RowsAffected()
		if err != nil {
			return err
		}
		if count != 1 {
			return domain.ErrConflict
		}
	}
	return nil
}
