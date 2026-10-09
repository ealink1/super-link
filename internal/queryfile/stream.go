package queryfile

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"github.com/ealink1/super-link/internal/upstream/sqlparam"
	"io"
	"strings"
	"unicode/utf8"
)

// Statements reads one statement at a time with no total file-size limit.
// Memory is proportional to one SQL statement, rather than the whole file.
func Statements(ctx context.Context, input io.Reader, dialect string, use func(string, int64) error) (string, error) {
	hash := sha256.New()
	reader := bufio.NewReaderSize(io.TeeReader(input, hash), 64<<10)
	state := sqlFileLexState{}
	var offset int64
	opts := sqlparam.OptionsForDBType(dialect)
	mysql := opts.HashComments
	var previous byte
	read := func() (byte, error) {
		b, err := reader.ReadByte()
		if err == nil {
			offset++
		}
		return b, err
	}
	emit := func() error {
		if err := ctx.Err(); err != nil {
			return err
		}
		text := strings.TrimSpace(strings.TrimPrefix(state.statement.String(), "\ufeff"))
		state.statement.Reset()
		if text == "" {
			return nil
		}
		if !utf8.ValidString(text) || strings.ContainsRune(text, 0) {
			return errors.New("SQL 文件必须是 UTF-8 文本，且不能包含 NUL 字符")
		}
		return use(text, offset)
	}
	for {
		if offset&4095 == 0 {
			if err := ctx.Err(); err != nil {
				return "", err
			}
		}
		b, err := read()
		if err == io.EOF {
			break
		}
		if err != nil {
			return "", err
		}
		next, _ := reader.Peek(1)
		handled, lexErr := state.consume(b, next, reader, read)
		if lexErr != nil {
			return "", lexErr
		}
		if handled {
			continue
		}
		if b == '-' && len(next) > 0 && next[0] == '-' {
			peek, _ := reader.Peek(2)
			if !mysql || len(peek) < 2 || peek[1] <= ' ' {
				_, _ = read()
				state.lineComment = true
				state.statement.WriteByte(' ')
				continue
			}
		}
		if mysql && b == '#' {
			state.lineComment = true
			state.statement.WriteByte(' ')
			continue
		}
		if b == '/' && len(next) > 0 && next[0] == '*' {
			peek, _ := reader.Peek(2)
			if len(peek) > 1 && peek[1] == '!' {
				return "", errors.New("暂不支持 MySQL 可执行注释，请使用普通 SQL 语句")
			}
			_, _ = read()
			state.depth = 1
			state.statement.WriteByte(' ')
			continue
		}
		if b == ';' {
			if err := emit(); err != nil {
				return "", err
			}
			continue
		}
		if b == '\'' || b == '"' || b == '`' {
			state.quote = b
			state.escapedQuote = opts.BackslashEscapes || b == '\'' && opts.DollarQuotes && (previous == 'e' || previous == 'E')
		}
		if b == '[' && dialect == "sqlserver" {
			state.quote = ']'
		}
		if b == '$' && opts.DollarQuotes {
			for size := 1; ; size++ {
				peek, e := reader.Peek(size)
				if e != nil {
					break
				}
				c := peek[size-1]
				if c == '$' {
					state.dollar = "$" + string(peek)
					state.statement.WriteString(state.dollar)
					for range peek {
						_, _ = read()
					}
					break
				}
				if !(c == '_' || c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || size > 1 && c >= '0' && c <= '9') {
					break
				}
			}
			if state.dollar != "" {
				continue
			}
		}
		state.statement.WriteByte(b)
		previous = b
	}
	if err := state.finish(emit); err != nil {
		return "", err
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}

type sqlFileLexState struct {
	statement                 strings.Builder
	quote                     byte
	dollar                    string
	depth                     int
	lineComment, escapedQuote bool
}

func (state *sqlFileLexState) consume(b byte, next []byte, reader *bufio.Reader, read func() (byte, error)) (bool, error) {
	if state.lineComment {
		if b == '\n' {
			state.lineComment = false
			state.statement.WriteByte(' ')
		}
		return true, nil
	}
	if state.depth > 0 {
		if b == '/' && len(next) > 0 && next[0] == '*' {
			_, _ = read()
			state.depth++
		}
		if b == '*' && len(next) > 0 && next[0] == '/' {
			_, _ = read()
			state.depth--
			if state.depth == 0 {
				state.statement.WriteByte(' ')
			}
		}
		return true, nil
	}
	if state.dollar != "" {
		state.statement.WriteByte(b)
		if b == '$' {
			tail, _ := reader.Peek(len(state.dollar) - 1)
			if string(tail) == state.dollar[1:] {
				for range tail {
					c, _ := read()
					state.statement.WriteByte(c)
				}
				state.dollar = ""
			}
		}
		return true, nil
	}
	if state.quote != 0 {
		state.statement.WriteByte(b)
		if b == '\\' && state.quote != '`' && state.quote != ']' && state.escapedQuote {
			c, e := read()
			if e != nil {
				return true, errors.New("SQL 字符串未闭合")
			}
			state.statement.WriteByte(c)
			return true, nil
		}
		if b == state.quote {
			if len(next) > 0 && next[0] == state.quote {
				c, _ := read()
				state.statement.WriteByte(c)
			} else {
				state.quote = 0
			}
		}
		return true, nil
	}

	return false, nil
}

func (state *sqlFileLexState) finish(emit func() error) error {
	if state.quote != 0 || state.dollar != "" || state.depth != 0 {
		return errors.New("SQL 文件包含未闭合的字符串或注释")
	}
	if err := emit(); err != nil {
		return err
	}
	return nil
}
