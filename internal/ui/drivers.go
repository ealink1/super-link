package ui

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/drivers"
)

func (w *Window) driverManager() {
	label := widget.NewLabel("正在核验本地驱动摘要…")
	label.Wrapping = fyne.TextWrapWord
	selected := widget.NewSelect([]string{}, nil)
	view := container.NewVBox(label, selected)
	modal := dialog.NewCustom("独立驱动管理", "关闭", view, w.Window)
	modal.Resize(fyne.NewSize(900, 550))
	modal.Show()
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Drivers.Statuses(ctx), ctx.Err() }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		statuses := value.([]drivers.Status)
		names := []string{}
		for _, status := range statuses {
			names = append(names, status.Descriptor.Name)
		}
		selected.Options = names
		selected.Refresh()
		selected.OnChanged = func(name string) {
			for _, status := range statuses {
				if name == status.Descriptor.Name {
					label.SetText(status.Descriptor.Name + "\n" + status.Message)
				}
			}
		}
		if len(names) > 0 {
			selected.SetSelected(names[0])
		}
	})
	install := widget.NewButton("从签名 Release 安装 / 修复", func() {
		kind := ""
		for _, descriptor := range domain.Catalog() {
			if descriptor.Name == selected.Selected {
				kind = descriptor.Key
			}
		}
		if kind == "" {
			return
		}
		label.SetText("正在检查签名发布、下载并核验驱动…")
		profiles := append([]domain.Profile(nil), w.profiles...)
		for _, p := range profiles {
			if p.Config.Type == kind {
				w.cancelProfile(p.ID)
			}
		}
		w.jobs.run(func(ctx context.Context) (any, error) {
			ctx, cancel := context.WithTimeout(ctx, 5*time.Minute)
			defer cancel()
			manifest, err := w.Releases.Check(ctx)
			if err != nil {
				return nil, err
			}
			for _, profile := range profiles {
				if profile.Config.Type == kind {
					if err = w.Engine.Disconnect(ctx, profile.ID); err != nil {
						return nil, err
					}
				}
			}
			return nil, w.Drivers.InstallRelease(ctx, w.Releases, manifest, kind, filepath.Join(w.Root, "downloads"))
		}, func(_ any, err error) {
			if err != nil {
				label.SetText(updateCheckError(err).Error())
				return
			}
			label.SetText(fmt.Sprintf("%s 安装完成；兼容版本、协议和摘要均已校验。", selected.Selected))
		})
	})
	view.Add(install)
	view.Add(widget.NewButton("导入本机可信驱动包", func() {
		w.chooseLocalPath("导入本机可信驱动包", true, nil, func(directory string) {
			showConfirmDialog("导入本机驱动", "将执行此目录中的原生驱动程序。请确认来源可信：\n"+directory, func(ok bool) {
				if !ok {
					return
				}
				profiles := append([]domain.Profile(nil), w.profiles...)
				for _, profile := range profiles {
					w.cancelProfile(profile.ID)
				}
				label.SetText("正在核验本机驱动包…")
				w.jobs.run(func(ctx context.Context) (any, error) {
					ctx, cancel := context.WithTimeout(ctx, 5*time.Minute)
					defer cancel()
					for _, profile := range profiles {
						if err := w.Engine.Disconnect(ctx, profile.ID); err != nil {
							return nil, err
						}
					}
					if _, err := os.Stat(filepath.Join(directory, "bundle.json")); err != nil {
						return nil, fmt.Errorf("目录缺少 bundle.json：%w", err)
					}
					return nil, w.Drivers.InstallBundled(ctx, directory)
				}, func(_ any, err error) {
					if err != nil {
						label.SetText(err.Error())
						return
					}
					label.SetText("本机驱动包安装完成。重新打开驱动管理可查看最新状态。")
				})
			}, w.Window)
		})
	}))
	view.Add(widget.NewLabel("应用包自带 SQLite。其他驱动可从本机 bundle.json 目录或签名 Release 安装。"))
}
