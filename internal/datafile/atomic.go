package datafile

import (
	"context"
	"errors"
	"fmt"
	"github.com/ealink1/super-link/internal/infra/filevisibility"
	"os"
	"path/filepath"
	"strings"
)

// SaveAtomic keeps an existing file intact until every exported row is written.
func SaveAtomic(ctx context.Context, path string, overwrite bool, options Options, produce func(*Encoder) error) (int64, error) {
	if strings.TrimSpace(path) == "" {
		return 0, errors.New("choose an export file")
	}
	path, err := filepath.Abs(path)
	if err != nil {
		return 0, err
	}
	if !overwrite {
		if _, err = os.Lstat(path); err == nil {
			return 0, os.ErrExist
		}
		if !errors.Is(err, os.ErrNotExist) {
			return 0, err
		}
	}
	file, err := os.CreateTemp(filepath.Dir(path), ".superlink-export-*")
	if err != nil {
		return 0, err
	}
	defer os.Remove(file.Name())
	encoder, err := NewEncoder(ctx, file, options)
	if err != nil {
		_ = file.Close()
		return 0, err
	}
	defer encoder.Close()
	if err = produce(encoder); err == nil {
		err = encoder.Finish()
	}
	if err == nil {
		err = ctx.Err()
	}
	if err == nil {
		err = file.Sync()
	}
	err = errors.Join(err, file.Close())
	if err == nil {
		err = ctx.Err()
	}
	if err != nil {
		return encoder.Count, fmt.Errorf("export failed; destination preserved: %w", err)
	}
	if err = filevisibility.Prepare(file.Name()); err != nil {
		return encoder.Count, err
	}
	if overwrite {
		err = os.Rename(file.Name(), path)
	} else {
		err = os.Link(file.Name(), path)
	}
	if err != nil {
		return encoder.Count, fmt.Errorf("publish export file: %w", err)
	}
	return encoder.Count, nil
}
