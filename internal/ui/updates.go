package ui

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"time"

	"fyne.io/fyne/v2/dialog"
	"github.com/ealink1/super-link/internal/infra/release"
	"github.com/ealink1/super-link/internal/infra/update"
)

// CheckUpdatesOnStartup schedules one quiet check after workspace restoration.
func (w *Window) CheckUpdatesOnStartup() {
	if w.startupUpdateScheduled {
		return
	}
	w.startupUpdateScheduled = true
	w.jobs.run(func(ctx context.Context) (any, error) {
		select {
		case <-w.ready:
			return nil, nil
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}, func(_ any, err error) {
		if err == nil {
			w.checkUpdatesWithMode(true)
		}
	})
}

func (w *Window) checkUpdates() { w.checkUpdatesWithMode(false) }

func (w *Window) checkUpdatesWithMode(quiet bool) {
	if w.Releases == nil || w.updateChecking {
		return
	}
	w.updateChecking = true
	if !quiet {
		w.status.SetText("正在检查 GitHub Release…")
	}
	w.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 60*time.Second)
		defer cancel()
		return w.Releases.Check(ctx)
	}, func(value any, err error) {
		w.updateChecking = false
		if quiet && err != nil {
			return
		}
		if errors.Is(err, release.ErrNoRelease) {
			w.status.SetText("目标仓库尚未发布公开稳定版本（草稿和预发布不参与更新）")
			dialog.ShowInformation("检查更新", "暂无可用的公开稳定版本。Release 草稿和预发布不会用于应用更新。", w.Window)
			return
		}
		if err != nil {
			w.showError(updateCheckError(err))
			w.status.SetText("更新检查失败")
			return
		}
		manifest := value.(release.Manifest)
		if !release.Newer(manifest.Version, w.Version) {
			if quiet {
				return
			}
			w.status.SetText("当前已是最新稳定版本")
			dialog.ShowInformation("检查更新", "当前已是最新稳定版本。", w.Window)
			return
		}
		artifact, err := manifest.Artifact("app", "superlink", runtime.GOOS, runtime.GOARCH)
		if err != nil {
			if quiet {
				return
			}
			w.showError(err)
			w.status.SetText("更新检查失败")
			return
		}
		w.status.SetText("发现新版本 " + manifest.Version)
		dialog.ShowConfirm("发现新版本", fmt.Sprintf("%s → %s\n签名清单验证通过。下载完整应用包（含配套驱动），大小 %.1f MiB？", w.Version, manifest.Version, float64(artifact.Size)/(1<<20)), func(ok bool) {
			if !ok {
				return
			}
			w.downloadUpdate(artifact, manifest.Version)
		}, w.Window)
	})
}

func updateCheckError(err error) error {
	if errors.Is(err, release.ErrNoRelease) {
		return fmt.Errorf("暂无可用的公开稳定版本，Release 草稿和预发布不参与更新：%w", err)
	}
	var status *release.HTTPError
	if !errors.As(err, &status) {
		return err
	}
	if status.StatusCode == http.StatusForbidden || status.StatusCode == http.StatusTooManyRequests {
		reason := "GitHub 拒绝了更新请求"
		if status.RateLimited {
			reason = "GitHub 更新请求已被限流"
		}
		return fmt.Errorf("%s。请稍后重试，并检查网络或代理是否允许访问 api.github.com 和 github.com。\n详细信息：%w", reason, err)
	}
	if status.StatusCode == http.StatusNotFound {
		return fmt.Errorf("更新发布缺少可下载的签名清单，请等待维护者补齐发布文件。\n详细信息：%w", err)
	}
	return err
}
func (w *Window) installUpdate(path, version string) {
	executable, err := os.Executable()
	if err != nil {
		w.showError(err)
		return
	}
	target, err := update.RunningRoot(executable)
	if err != nil {
		dialog.ShowInformation("更新包已校验", err.Error()+"\n下载位置：\n"+path, w.Window)
		return
	}
	dialog.ShowConfirm("安装并重启", "完整应用包已校验。保存草稿后退出并替换应用；旧版本会保留为备份。是否继续？", func(ok bool) {
		if !ok {
			return
		}
		w.status.SetText("正在准备应用替换…")
		w.jobs.run(func(ctx context.Context) (any, error) {
			return update.Prepare(ctx, path, filepath.Dir(path), target, version, w.Root)
		}, func(value any, err error) {
			if err != nil {
				w.showError(err)
				return
			}
			request := value.(update.Request)
			w.pendingUpdate = &request
			w.shutdown()
		})
	}, w.Window)
}
