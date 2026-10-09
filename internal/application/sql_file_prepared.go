package application

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io"
	"os"
	"sync"
	"time"

	"github.com/ealink1/super-link/internal/domain"
)

// PreparedSQLFile records a checked file identity. Execute is an explicit single-use
// confirmation; Close invalidates a preparation that the user cancels.
type PreparedSQLFile struct {
	info    SQLFileInfo
	engine  *Engine
	profile domain.Profile
	scope   string
	stat    os.FileInfo
	mu      sync.Mutex
	path    string
	used    bool
}

func (e *Engine) PrepareSQLFile(ctx context.Context, id, scope, path string, revision int64, progress func(SQLFileProgress)) (*PreparedSQLFile, error) {
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return nil, err
	}
	if p.Revision != revision {
		return nil, domain.ErrConflict
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	before, err := file.Stat()
	if err != nil {
		return nil, err
	}
	info, err := inspectSQLFileProgress(ctx, file, p, func(state SQLFileProgress) {
		state.Phase = "统计语句"
		if progress != nil {
			progress(state)
		}
	})
	if err != nil {
		return nil, err
	}
	if _, err = file.Seek(0, 0); err != nil {
		return nil, err
	}
	hash := sha256.New()
	state := SQLFileProgress{Phase: "核对摘要", Checked: info.Statements, Total: info.Statements, TotalBytes: info.Bytes}
	report := func() {
		if progress != nil {
			progress(state)
		}
	}
	report()
	buffer := make([]byte, 256<<10)
	last := time.Now()
	for {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		n, readErr := file.Read(buffer)
		if n > 0 {
			hash.Write(buffer[:n])
			state.Bytes += int64(n)
		}
		if time.Since(last) >= 100*time.Millisecond {
			report()
			last = time.Now()
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return nil, readErr
		}
	}
	report()
	after, err := file.Stat()
	if err != nil {
		return nil, err
	}
	if hex.EncodeToString(hash.Sum(nil)) != info.Digest || !sameSQLFileStat(before, after) {
		return nil, errors.New("SQL 文件已修改，请重新选择并检查")
	}
	if _, err = file.Seek(0, 0); err != nil {
		return nil, err
	}
	if err = e.checkRevision(ctx, id, revision); err != nil {
		return nil, err
	}
	return &PreparedSQLFile{info: info, engine: e, profile: p, scope: scope, stat: after, path: path}, nil
}

func sameSQLFileStat(a, b os.FileInfo) bool {
	return os.SameFile(a, b) && a.Size() == b.Size() && a.ModTime().Equal(b.ModTime())
}

func (p *PreparedSQLFile) Info() SQLFileInfo { return p.info }

func (p *PreparedSQLFile) Close() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.used = true
	return nil
}

func (p *PreparedSQLFile) Execute(ctx context.Context, confirmed bool, progress func(SQLFileProgress)) (int64, error) {
	if !confirmed {
		return 0, errors.New("请先确认执行 SQL 文件")
	}
	p.mu.Lock()
	if p.used {
		p.mu.Unlock()
		return 0, domain.ErrClosed
	}
	p.used = true
	p.mu.Unlock()
	file, err := os.Open(p.path)
	if err != nil {
		return 0, err
	}
	defer file.Close()
	current, err := p.engine.Profiles.Get(ctx, p.profile.ID)
	if err != nil {
		return 0, err
	}
	if current.Revision != p.profile.Revision {
		return 0, domain.ErrConflict
	}
	if p.info.Write && (current.ReadOnly || current.Config.Protection.RestrictScriptExecution) {
		return 0, domain.ErrReadOnly
	}
	stat, err := file.Stat()
	if err != nil {
		return 0, err
	}
	if !sameSQLFileStat(p.stat, stat) {
		return 0, errors.New("确认期间 SQL 文件已修改，请重新选择并检查")
	}
	return p.engine.executeCheckedSQLFile(ctx, current, current.ID, p.scope, file, p.info, p.stat, progress)
}
