package update

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/ealink1/super-link/internal/infra/instance"
	"github.com/shirou/gopsutil/v4/process"
)

type Report struct {
	Version    string    `json:"version"`
	Success    bool      `json:"success"`
	RolledBack bool      `json:"rolledBack"`
	Backup     string    `json:"backup"`
	Message    string    `json:"message"`
	ProcessID  int       `json:"processId,omitempty"`
	Time       time.Time `json:"time"`
}

func Launch(ctx context.Context, r Request) error {
	if err := ValidateRequest(r); err != nil {
		return err
	}
	p, err := ReadPackage(r.Stage)
	if err != nil {
		return err
	}
	temporary, err := os.MkdirTemp(filepath.Dir(r.HealthFile), "helper-*")
	if err != nil {
		return err
	}
	name := filepath.Base(p.Helper)
	input, err := os.ReadFile(filepath.Join(r.Stage, filepath.FromSlash(p.Helper)))
	if err != nil {
		return err
	}
	helper := filepath.Join(temporary, name)
	if err = os.WriteFile(helper, input, 0700); err != nil {
		return err
	}
	requestPath := filepath.Join(temporary, "request.json")
	raw, err := json.Marshal(r)
	if err != nil {
		return err
	}
	if err = os.WriteFile(requestPath, raw, 0600); err != nil {
		return err
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	command := helperCommand(helper, requestPath)
	if err = command.Start(); err != nil {
		return err
	}
	return command.Process.Release()
}

// Keep the helper outside the installed directory. Windows prevents renaming
// a directory used as any running process's current working directory.
func helperCommand(helper, requestPath string) *exec.Cmd {
	command := exec.Command(helper, "--request", requestPath)
	command.Dir = filepath.Dir(helper)
	configureUpdateProcess(command)
	return command
}

// ApplyWithHealth rolls back both a failed rename and a failed first launch.
// A backup is retained after success for manual recovery; it is never deleted
// automatically while the new version may still need it.
func ApplyWithHealth(r Request, launch func(Request) error) Report {
	report := Report{Version: r.Version, Backup: r.Backup, Time: time.Now().UTC()}
	if err := ValidateRequest(r); err != nil {
		report.Message = err.Error()
		return report
	}
	lock, err := instance.Acquire(r.DataRoot)
	if err != nil {
		report.Message = err.Error()
		return report
	}
	defer func() { _ = lock.Close() }()
	if err = backupState(r); err != nil {
		report.Message = "local state backup failed: " + err.Error()
		return report
	}
	if err := os.Rename(r.Target, r.Backup); err != nil {
		report.Message = err.Error()
		return report
	}
	if err := os.Rename(r.Stage, r.Target); err != nil {
		rollbackErr := os.Rename(r.Backup, r.Target)
		report.RolledBack = rollbackErr == nil
		report.Message = errors.Join(err, rollbackErr).Error()
		return report
	}
	if err := lock.Close(); err != nil {
		report.Message = err.Error()
		return report
	}
	if err := launch(r); err != nil {
		lock, lockErr := instance.Acquire(r.DataRoot)
		if lockErr != nil {
			report.Message = errors.Join(err, lockErr).Error()
			return report
		}
		defer lock.Close()
		stateErr := restoreState(r)
		failed := r.Stage + ".failed"
		moveErr := os.Rename(r.Target, failed)
		var rollbackErr error
		if moveErr == nil {
			rollbackErr = os.Rename(r.Backup, r.Target)
		}
		report.RolledBack = stateErr == nil && moveErr == nil && rollbackErr == nil
		report.Message = errors.Join(err, stateErr, moveErr, rollbackErr).Error()
		return report
	}
	report.Success = true
	report.Message = "Application replaced and first-launch health confirmed"
	return report
}
func WaitParent(ctx context.Context, pid int) error {
	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()
	for {
		exists, err := process.PidExistsWithContext(ctx, int32(pid))
		if err != nil {
			return err
		}
		if !exists {
			return nil
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
}
func StartAndCheck(ctx context.Context, r Request) error {
	p, err := ReadPackage(r.Target)
	if err != nil {
		return err
	}
	_ = os.Remove(r.HealthFile)
	command := exec.Command(filepath.Join(r.Target, filepath.FromSlash(p.Executable)), "--data-root", r.DataRoot, "--health-file", r.HealthFile, "--health-token", r.Token)
	if err = command.Start(); err != nil {
		return err
	}
	exited := make(chan error, 1)
	go func() { exited <- command.Wait() }()
	timer := time.NewTicker(100 * time.Millisecond)
	defer timer.Stop()
	for {
		select {
		case err := <-exited:
			return fmt.Errorf("new application exited before health confirmation: %v", err)
		case <-ctx.Done():
			_ = command.Process.Kill()
			<-exited
			return ctx.Err()
		case <-timer.C:
			raw, err := os.ReadFile(r.HealthFile)
			if err != nil {
				continue
			}
			var health struct {
				Token   string `json:"token"`
				Version string `json:"version"`
				PID     int    `json:"pid"`
			}
			if json.Unmarshal(raw, &health) == nil && health.Token == r.Token && health.Version == r.Version && health.PID == command.Process.Pid {
				return nil
			}
		}
	}
}
func MarkHealthy(path, token, version string) error {
	if path == "" {
		return nil
	}
	raw, _ := json.Marshal(struct {
		Token   string `json:"token"`
		Version string `json:"version"`
		PID     int    `json:"pid"`
	}{token, version, os.Getpid()})
	file, err := os.CreateTemp(filepath.Dir(path), ".health-*")
	if err != nil {
		return err
	}
	temporary := file.Name()
	defer os.Remove(temporary)
	if _, err = file.Write(raw); err != nil {
		_ = file.Close()
		return err
	}
	if err = file.Close(); err != nil {
		return err
	}
	return os.Rename(temporary, path)
}
func WriteReport(path string, report Report) error {
	raw, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, raw, 0600)
}
