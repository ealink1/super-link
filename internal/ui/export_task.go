package ui

import (
	"context"
	"fmt"
	"time"

	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/datafile"
)

func (w *Window) startExport(source exportSource, options datafile.Options, path string, overwrite, full bool) {
	progress := widget.NewProgressBarInfinite()
	status := widget.NewLabel("正在导出…")
	var cancel context.CancelFunc
	modal := dialog.NewCustom("导出数据", "停止", container.NewVBox(progress, status), w.Window)
	modal.SetOnClosed(func() {
		if cancel != nil {
			cancel()
		}
	})
	modal.Show()
	w.jobs.runWithCancel(&cancel, func(ctx context.Context) (any, error) {
		ctx, stop := context.WithTimeout(ctx, 10*time.Minute)
		defer stop()
		return datafile.SaveAtomic(ctx, path, overwrite, options, func(encoder *datafile.Encoder) error {
			if full {
				return w.Engine.StreamQuery(ctx, source.profile.ID, source.request, encoder)
			}
			columns := []string{}
			for _, c := range source.result.Columns {
				columns = append(columns, c.Name)
			}
			if err := encoder.SetColumns(columns); err != nil {
				return err
			}
			for _, row := range source.result.Rows {
				if err := encoder.ConsumeRowValues(row); err != nil {
					return err
				}
			}
			return nil
		})
	}, func(value any, err error) {
		modal.Hide()
		if err != nil {
			w.showError(err)
			return
		}
		dialog.ShowInformation("导出完成", fmt.Sprintf("已导出 %d 行\n%s", value.(int64), path), w.Window)
	})
}
