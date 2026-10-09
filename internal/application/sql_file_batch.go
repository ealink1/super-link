package application

import (
	"context"
	"io"
	"regexp"
	"strings"

	"github.com/ealink1/super-link/internal/queryfile"
)

// Only literal VALUES inserts with quoted identifiers are merged. Expressions,
// conflict clauses and other statements keep their original execution boundary.
var importInsert = regexp.MustCompile("^INSERT INTO ((?:`[^`]+`\\.)?`[^`]+` \\((?:`[^`]+`(?:, )?)+\\)) VALUES (\\([\\s\\S]*\\))$")
var importLiterals = regexp.MustCompile(`^\((?:NULL|-?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?|0x[0-9a-fA-F]+|X'[0-9a-fA-F]*'|CONVERT\(X'[0-9a-fA-F]*' USING utf8mb4\)|'(?:[^'\\]|\\[\s\S]|'')*')(?:, (?:NULL|-?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?|0x[0-9a-fA-F]+|X'[0-9a-fA-F]*'|CONVERT\(X'[0-9a-fA-F]*' USING utf8mb4\)|'(?:[^'\\]|\\[\s\S]|'')*'))*\)$`)

func executeSQLFileStatements(ctx context.Context, input io.Reader, dialect string, execute func(string, int64, int64) error) error {
	enabled := dialect == "mysql" || dialect == "mariadb"
	var prefix string
	var batch strings.Builder
	var count, offset int64
	flush := func() error {
		if count == 0 {
			return nil
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		err := execute(batch.String(), offset, count)
		batch.Reset()
		count = 0
		prefix = ""
		return err
	}
	_, err := queryfile.Statements(ctx, input, dialect, func(text string, position int64) error {
		var parts []string
		if enabled {
			parts = importInsert.FindStringSubmatch(text)
		}
		if len(parts) == 0 || !importLiterals.MatchString(parts[2]) {
			if err := flush(); err != nil {
				return err
			}
			return execute(text, position, 1)
		}
		head := "INSERT INTO " + parts[1] + " VALUES "
		if count > 0 && (head != prefix || count >= 128 || batch.Len()+len(parts[2]) > 512<<10) {
			if err := flush(); err != nil {
				return err
			}
		}
		if count == 0 {
			prefix = head
			batch.WriteString(head)
		} else {
			batch.WriteString(", ")
		}
		batch.WriteString(parts[2])
		count++
		offset = position
		return nil
	})
	if err != nil {
		return err
	}
	return flush()
}
