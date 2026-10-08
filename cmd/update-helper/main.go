package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/ealink1/super-link/internal/infra/update"
)

func main() {
	requestPath := flag.String("request", "", "private update request file")
	flag.Parse()
	if *requestPath == "" {
		os.Exit(2)
	}
	raw, err := os.ReadFile(*requestPath)
	if err != nil || len(raw) > 16384 {
		os.Exit(2)
	}
	var request update.Request
	if err = json.Unmarshal(raw, &request); err != nil {
		os.Exit(2)
	}
	if err = update.ValidateRequest(request); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()
	if err = update.WaitParent(ctx, request.ParentPID); err != nil {
		_ = update.WriteReport(request.Report, update.Report{Version: request.Version, Message: err.Error(), Time: time.Now()})
		return
	}
	report := update.ApplyWithHealth(request, func(r update.Request) error {
		healthCtx, healthCancel := context.WithTimeout(ctx, 45*time.Second)
		defer healthCancel()
		return update.StartAndCheck(healthCtx, r)
	})
	if report.Success {
		if raw, err := os.ReadFile(request.HealthFile); err == nil {
			var health struct {
				PID int `json:"pid"`
			}
			if json.Unmarshal(raw, &health) == nil {
				report.ProcessID = health.PID
			}
		}
	}
	_ = update.WriteReport(request.Report, report)
	if !report.Success {
		if p, err := update.ReadPackage(request.Target); err == nil && p.Version != request.Version {
			command := exec.Command(filepath.Join(request.Target, filepath.FromSlash(p.Executable)), "--data-root", request.DataRoot)
			command.Dir = request.Target
			if command.Start() == nil {
				_ = command.Process.Release()
			}
		}
	}
	_ = os.Remove(request.HealthFile)
}
