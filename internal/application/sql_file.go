package application

import (
	"context"
	"errors"
	"fmt"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/queryfile"
	"os"
	"time"
)

type SQLFileInfo struct {
	Digest            string
	Statements, Bytes int64
	Write             bool
}
type SQLFileProgress struct {
	Phase                                                 string
	Completed, Checked, Current, Total, Bytes, TotalBytes int64
}

func (e *Engine) InspectSQLFile(ctx context.Context, id, path string, revision int64) (SQLFileInfo, error) {
	return e.InspectSQLFileWithProgress(ctx, id, path, revision, nil)
}

func (e *Engine) InspectSQLFileWithProgress(ctx context.Context, id, path string, revision int64, progress func(SQLFileProgress)) (SQLFileInfo, error) {
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return SQLFileInfo{}, err
	}
	if p.Revision != revision {
		return SQLFileInfo{}, domain.ErrConflict
	}
	file, err := os.Open(path)
	if err != nil {
		return SQLFileInfo{}, err
	}
	defer file.Close()
	return inspectSQLFileProgress(ctx, file, p, progress)
}
func inspectSQLFile(ctx context.Context, file *os.File, p domain.Profile) (SQLFileInfo, error) {
	return inspectSQLFileProgress(ctx, file, p, nil)
}
func inspectSQLFileProgress(ctx context.Context, file *os.File, p domain.Profile, progress func(SQLFileProgress)) (SQLFileInfo, error) {
	result := SQLFileInfo{}
	descriptor, err := domain.Resolve(p.Config.Type)
	if err != nil {
		return result, err
	}
	if descriptor.Family != domain.SQL {
		return result, errors.New("当前数据源不支持 SQL 文件")
	}

	stat, err := file.Stat()
	if err != nil {
		return result, err
	}
	if !stat.Mode().IsRegular() {
		return result, errors.New("请选择普通 SQL 文件")
	}
	result.Bytes = stat.Size()
	state := SQLFileProgress{Phase: "核对文件", TotalBytes: result.Bytes}
	var last time.Time
	report := func(force bool) {
		if progress != nil && (force || time.Since(last) >= 100*time.Millisecond) {
			last = time.Now()
			progress(state)
		}
	}
	report(true)
	input := &sqlFileProgressReader{Reader: file, update: func(bytes int64) {
		state.Bytes = bytes
		report(false)
	}}
	result.Digest, err = queryfile.Statements(ctx, input, p.SQLDialect(), func(text string, _ int64) error {
		write, err := classifySQL(text, p.SQLDialect())
		if err != nil {
			return fmt.Errorf("第 %d 条 SQL：%w", result.Statements+1, err)
		}
		result.Write = result.Write || write
		result.Statements++
		state.Checked = result.Statements
		report(false)
		return nil
	})
	if err != nil {
		return result, err
	}
	state.Bytes = result.Bytes
	report(true)
	if result.Statements == 0 {
		return result, errors.New("SQL 文件没有可执行语句")
	}
	if result.Write && (p.ReadOnly || p.Config.Protection.RestrictScriptExecution) {
		return result, domain.ErrReadOnly
	}
	return result, nil
}

// ExecuteSQLFile holds one session lease for the file, preserving SET/USE state.
// It never loads the whole file or keeps query result sets between statements.
func (e *Engine) ExecuteSQLFile(ctx context.Context, id, scope, path string, revision int64, expected SQLFileInfo, confirmation string, progress func(SQLFileProgress)) (int64, error) {
	p, err := e.Profiles.Get(ctx, id)
	if err != nil {
		return 0, err
	}
	if p.Revision != revision {
		return 0, domain.ErrConflict
	}
	file, err := os.Open(path)
	if err != nil {
		return 0, err
	}
	defer file.Close()
	info, err := inspectSQLFileProgress(ctx, file, p, progress)
	if err != nil {
		return 0, err
	}
	if info.Digest != expected.Digest {
		return 0, errors.New("SQL 文件已修改，请重新选择并确认")
	}
	if info.Write {
		err = e.authorize(p, domain.Execution{Text: "sql-file:" + info.Digest, Scope: scope, Confirmation: confirmation})
		if err != nil {
			return 0, err
		}
	}
	if _, err = file.Seek(0, 0); err != nil {
		return 0, err
	}
	verifiedStat, err := file.Stat()
	if err != nil {
		return 0, err
	}
	return e.executeCheckedSQLFile(ctx, p, id, scope, file, info, verifiedStat, progress)
}

func (e *Engine) executeCheckedSQLFile(ctx context.Context, p domain.Profile, id, scope string, file *os.File, info SQLFileInfo, verifiedStat os.FileInfo, progress func(SQLFileProgress)) (int64, error) {
	revision := p.Revision
	if progress != nil {
		progress(SQLFileProgress{Phase: "连接数据库", Checked: info.Statements, Total: info.Statements, TotalBytes: info.Bytes})
	}
	current, session, release, err := e.acquire(ctx, id, scope)
	if err != nil {
		return 0, err
	}
	defer release()
	if current.Revision != revision {
		return 0, domain.ErrConflict
	}
	before, err := file.Stat()
	if err != nil {
		return 0, err
	}
	if !sameSQLFileStat(verifiedStat, before) {
		return 0, errors.New("连接期间 SQL 文件已修改，请重新检查")
	}
	var completed int64
	var last time.Time
	err = executeSQLFileStatements(ctx, file, p.SQLDialect(), func(text string, offset, count int64) error {
		currentFile, err := file.Stat()
		if err != nil {
			return err
		}
		if currentFile.Size() != before.Size() || !currentFile.ModTime().Equal(before.ModTime()) {
			return errors.New("执行期间 SQL 文件被修改，已停止")
		}
		if err := e.checkRevision(ctx, id, revision); err != nil {
			return err
		}
		write, err := classifySQL(text, p.SQLDialect())
		if err != nil {
			return err
		}
		queryCtx := ctx
		cancel := func() {}
		if p.Config.QueryTimeout > 0 {
			queryCtx, cancel = context.WithTimeout(ctx, time.Duration(p.Config.QueryTimeout)*time.Second)
		}
		if progress != nil && (completed == 0 || time.Since(last) >= 100*time.Millisecond) {
			last = time.Now()
			progress(SQLFileProgress{Phase: "执行 SQL", Checked: info.Statements, Current: completed + 1, Completed: completed, Total: info.Statements, Bytes: offset, TotalBytes: info.Bytes})
		}
		_, err = session.client.Execute(queryCtx, domain.Execution{Text: text, Scope: scope, Revision: revision, Write: write, MaxRows: 1})
		cancel()
		if err != nil {
			return fmt.Errorf("第 %d–%d 条 SQL 执行失败（前 %d 条已完成；失败批次可能部分写入）：%w", completed+1, completed+count, completed, err)
		}
		completed += count
		if progress != nil && (completed == count || time.Since(last) >= 100*time.Millisecond || completed == info.Statements) {
			last = time.Now()
			progress(SQLFileProgress{Phase: "执行 SQL", Checked: info.Statements, Completed: completed, Total: info.Statements, Bytes: offset, TotalBytes: info.Bytes})
		}
		return nil
	})
	// Discard the session so imported SET/USE state cannot leak to later queries.
	_ = session.client.Close()
	session.client = nil
	session.setConnectionStatus(ConnectionDisconnected)
	auditCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	auditErr := e.Profiles.Store.AddHistory(auditCtx, domain.History{ProfileID: id, Text: "EXECUTE SQL FILE", Success: err == nil, Rows: completed})
	if err == nil {
		err = auditErr
	}
	return completed, err
}
