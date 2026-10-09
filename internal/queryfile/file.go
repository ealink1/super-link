// Package queryfile reads and atomically saves UTF-8 SQL documents.
package queryfile

import (
	"context"
	"errors"
	"github.com/ealink1/super-link/internal/infra/filevisibility"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode/utf8"
)

const MaxSize = 1 << 20

func validate(text string) error {
	if len(text) > MaxSize || !utf8.ValidString(text) || strings.ContainsRune(text, 0) {
		return errors.New("SQL file must be UTF-8 without NUL characters and no larger than 1 MiB")
	}
	return nil
}

func Read(ctx context.Context, path string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return "", err
	}
	if !info.Mode().IsRegular() || info.Size() > MaxSize+3 {
		return "", errors.New("choose a regular SQL file no larger than 1 MiB")
	}
	raw, err := io.ReadAll(io.LimitReader(file, MaxSize+4))
	if err != nil {
		return "", err
	}
	text := strings.TrimPrefix(string(raw), "\ufeff")
	if err = validate(text); err != nil {
		return "", err
	}
	return text, ctx.Err()
}

// Save publishes a complete private temporary file only after sync and close.
// A failed write or cancellation leaves the prior document untouched.
func Save(ctx context.Context, path, text string, overwrite bool) error {
	if err := validate(text); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if !filepath.IsAbs(path) {
		return errors.New("SQL file path must be absolute")
	}
	file, err := os.CreateTemp(filepath.Dir(path), ".superlink-sql-*")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	_, err = io.WriteString(file, text)
	if err == nil {
		err = file.Sync()
	}
	err = errors.Join(err, file.Close(), ctx.Err())
	if err != nil {
		return err
	}
	if err = filevisibility.Prepare(file.Name()); err != nil {
		return err
	}
	if overwrite {
		return os.Rename(file.Name(), path)
	}
	return os.Link(file.Name(), path)
}
