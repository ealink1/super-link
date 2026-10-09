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
	var statement strings.Builder
	var quote byte
	var dollar string
	var offset int64
	depth := 0
	lineComment := false
	opts := sqlparam.OptionsForDBType(dialect)
	mysql := opts.HashComments
	escapedQuote := false
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
		text := strings.TrimSpace(strings.TrimPrefix(statement.String(), "\ufeff"))
		statement.Reset()
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
		if lineComment {
			if b == '\n' {
				lineComment = false
				statement.WriteByte(' ')
			}
			continue
		}
		if depth > 0 {
			if b == '/' && len(next) > 0 && next[0] == '*' {
				_, _ = read()
				depth++
			}
			if b == '*' && len(next) > 0 && next[0] == '/' {
				_, _ = read()
				depth--
				if depth == 0 {
					statement.WriteByte(' ')
				}
			}
			continue
		}
		if dollar != "" {
			statement.WriteByte(b)
			if b == '$' {
				tail, _ := reader.Peek(len(dollar) - 1)
				if string(tail) == dollar[1:] {
					for range tail {
						c, _ := read()
						statement.WriteByte(c)
					}
					dollar = ""
				}
			}
			continue
		}
		if quote != 0 {
			statement.WriteByte(b)
			if b == '\\' && quote != '`' && quote != ']' && escapedQuote {
				c, e := read()
				if e != nil {
					return "", errors.New("SQL 字符串未闭合")
				}
				statement.WriteByte(c)
				continue
			}
			if b == quote {
				if len(next) > 0 && next[0] == quote {
					c, _ := read()
					statement.WriteByte(c)
				} else {
					quote = 0
				}
			}
			continue
		}
		if b == '-' && len(next) > 0 && next[0] == '-' {
			peek, _ := reader.Peek(2)
			if !mysql || len(peek) < 2 || peek[1] <= ' ' {
				_, _ = read()
				lineComment = true
				statement.WriteByte(' ')
				continue
			}
		}
		if mysql && b == '#' {
			lineComment = true
			statement.WriteByte(' ')
			continue
		}
		if b == '/' && len(next) > 0 && next[0] == '*' {
			peek, _ := reader.Peek(2)
			if len(peek) > 1 && peek[1] == '!' {
				return "", errors.New("暂不支持 MySQL 可执行注释，请使用普通 SQL 语句")
			}
			_, _ = read()
			depth = 1
			statement.WriteByte(' ')
			continue
		}
		if b == ';' {
			if err := emit(); err != nil {
				return "", err
			}
			continue
		}
		if b == '\'' || b == '"' || b == '`' {
			quote = b
			escapedQuote = opts.BackslashEscapes || b == '\'' && opts.DollarQuotes && (previous == 'e' || previous == 'E')
		}
		if b == '[' && dialect == "sqlserver" {
			quote = ']'
		}
		if b == '$' && opts.DollarQuotes {
			for size := 1; ; size++ {
				peek, e := reader.Peek(size)
				if e != nil {
					break
				}
				c := peek[size-1]
				if c == '$' {
					dollar = "$" + string(peek)
					statement.WriteString(dollar)
					for range peek {
						_, _ = read()
					}
					break
				}
				if !(c == '_' || c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || size > 1 && c >= '0' && c <= '9') {
					break
				}
			}
			if dollar != "" {
				continue
			}
		}
		statement.WriteByte(b)
		previous = b
	}
	if quote != 0 || dollar != "" || depth != 0 {
		return "", errors.New("SQL 文件包含未闭合的字符串或注释")
	}
	if err := emit(); err != nil {
		return "", err
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}
