package ui

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/infra/release"
)

func (w *Window) downloadUpdate(artifact release.Artifact, version string) {
	bar := widget.NewProgressBar()
	label := widget.NewLabel(updateDownloadText(0, artifact.Size))
	var cancel context.CancelFunc
	active := true
	content := container.NewStack()
	progressDialog := dialog.NewCustomWithoutButtons("", content, w.Window)
	cancelButton := widget.NewButton("取消下载", progressDialog.Hide)
	working := widget.NewButton("更新中…", nil)
	working.Importance = widget.HighImportance
	working.Disable()
	content.Add(updateCard(w.Version, version, artifact.Size, container.NewVBox(bar, label), container.NewGridWithColumns(2, cancelButton, working)))
	progressDialog.Resize(fyne.NewSize(440, 570))
	progressDialog.SetOnClosed(func() {
		if active && cancel != nil {
			cancel()
		}
	})
	progressDialog.Show()
	w.status.SetText("正在下载完整应用包…")
	w.jobs.runWithCancel(&cancel, func(ctx context.Context) (any, error) {
		ctx, timeoutCancel := context.WithTimeout(ctx, 10*time.Minute)
		defer timeoutCancel()
		var last time.Time
		return w.Releases.DownloadWithProgress(ctx, artifact, filepath.Join(w.Root, "updates", version), func(downloaded, total int64) {
			// Bound UI updates even when the network delivers many small chunks.
			if downloaded != 0 && downloaded != total && time.Since(last) < 100*time.Millisecond {
				return
			}
			last = time.Now()
			w.jobs.dispatch(func() {
				if !active || w.jobs.closing.Load() || ctx.Err() != nil {
					return
				}
				bar.SetValue(float64(downloaded) / float64(total))
				label.SetText(updateDownloadText(downloaded, total))
				if downloaded == total {
					label.SetText("下载完成，正在校验应用包…")
					working.SetText("校验中…")
					w.status.SetText("正在校验完整应用包…")
				}
			})
		})
	}, func(value any, err error) {
		active = false
		progressDialog.Hide()
		if errors.Is(err, context.Canceled) {
			w.status.SetText("更新下载已取消")
			return
		}
		if err != nil {
			w.status.SetText("更新下载失败")
			w.showError(err)
			return
		}
		w.status.SetText("完整应用包已下载并校验")
		w.installUpdate(value.(string), version)
	})
}

func updateDownloadText(downloaded, total int64) string {
	return fmt.Sprintf("正在下载：%.1f / %.1f MiB（%.0f%%）", float64(downloaded)/(1<<20), float64(total)/(1<<20), 100*float64(downloaded)/float64(total))
}
