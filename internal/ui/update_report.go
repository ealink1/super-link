package ui

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"fyne.io/fyne/v2/dialog"
	"github.com/ealink1/super-link/internal/infra/update"
	"os"
	"path/filepath"
)

func (w *Window) checkUpdateReport() {
	w.jobs.run(func(ctx context.Context) (any, error) {
		path := filepath.Join(w.Root, "updates", "last-update.json")
		info, err := os.Stat(path)
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		if err != nil {
			return nil, err
		}
		if info.Size() > 16384 {
			return nil, fmt.Errorf("update report too large")
		}
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		raw, err := os.ReadFile(path)
		if err != nil {
			return nil, err
		}
		var report update.Report
		if err = json.Unmarshal(raw, &report); err != nil {
			return nil, err
		}
		if err = os.Rename(path, path+".reviewed"); err != nil {
			return nil, err
		}
		return report, nil
	}, func(value any, err error) {
		if err != nil || value == nil {
			return
		}
		report := value.(update.Report)
		if report.Success {
			return
		}
		message := "应用更新未成功。"
		if report.RolledBack {
			message += "已恢复旧版本。"
		}
		dialog.ShowInformation("更新未成功", message+"\n目标版本："+report.Version+"\n原因："+report.Message, w.Window)
	})
}
