package ui

import (
	"context"
	"fmt"
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/databaseexport"
	"github.com/ealink1/super-link/internal/domain"
	"path/filepath"
	"strings"
	"time"
)

func (n *navigator) exportDatabase(node *navNode, includeData bool) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	title, description := "导出全部表结构 · SQL", "导出全部表的建表结构（不含数据）。"
	if includeData {
		title, description = "备份全部表 · 结构 + 数据 SQL", "导出全部表的建表结构和数据（SQL）。"
	}
	path := widget.NewEntry()
	path.SetPlaceHolder("选择输出 SQL 文件")
	browse := widget.NewButton("选择目录", func() {
		n.owner.chooseLocalPath("选择数据库导出目录", true, nil, func(directory string) {
			path.SetText(filepath.Join(directory, databaseExportFilename(node.scope, time.Now())))
		})
	})
	body := container.NewVBox(widget.NewLabel(description), widget.NewLabel("不包含视图、存储过程、触发器和用户权限。"), widget.NewLabel("导出期间请避免修改结构和数据；此导出不提供一致性快照。"), widget.NewForm(widget.NewFormItem("输出文件", container.NewBorder(nil, nil, nil, browse, path))))
	modal := dialog.NewCustomConfirm(title+" · "+node.scope, "导出", "取消", body, func(ok bool) {
		if ok {
			n.runDatabaseExport(p, node.scope, path.Text, includeData)
		}
	}, n.owner.Window)
	modal.Resize(fyne.NewSize(620, 260))
	modal.Show()
}

func databaseExportFilename(scope string, now time.Time) string {
	name := strings.Map(func(r rune) rune {
		if r < 32 || strings.ContainsRune(`/\:*?"<>|`, r) {
			return '_'
		}
		return r
	}, scope)
	name = strings.Trim(name, " .")
	if name == "" {
		name = "database"
	}
	return name + "-" + now.In(time.FixedZone("Asia/Shanghai", 8*60*60)).Format("20060102-150405") + ".sql"
}

func (n *navigator) runDatabaseExport(p domain.Profile, scope, path string, includeData bool) {
	status := "正在导出表结构…"
	if includeData {
		status = "正在导出表结构与数据…"
	}
	label := widget.NewLabel(status)
	label.Wrapping = fyne.TextWrapWord
	bar := widget.NewProgressBar()
	active := true
	var cancel context.CancelFunc
	progress := dialog.NewCustom("导出数据库", "停止", container.NewVBox(label, bar), n.owner.Window)
	progress.Resize(fyne.NewSize(520, 240))
	progress.SetOnClosed(func() {
		active = false
		if cancel != nil {
			cancel()
		}
	})
	progress.Show()
	n.owner.jobs.runWithCancel(&cancel, func(ctx context.Context) (any, error) {
		ctx, stop := context.WithTimeout(ctx, 30*time.Minute)
		defer stop()
		current, err := n.owner.Profiles.Get(ctx, p.ID)
		if err != nil {
			return nil, err
		}
		if current.Revision != p.Revision {
			return nil, domain.ErrConflict
		}
		return databaseexport.SaveWithProgress(ctx, n.owner.Engine, p, scope, path, includeData, func(state databaseexport.Progress) {
			n.owner.jobs.dispatch(func() {
				if !active || n.owner.jobs.closing.Load() || ctx.Err() != nil {
					return
				}
				label.SetText(databaseExportProgressText(state))
				bar.SetValue(state.Fraction())
			})
		})
	}, func(value any, err error) {
		active = false
		progress.Hide()
		if err != nil {
			n.owner.showError(err)
			return
		}
		result := value.(databaseexport.Result)
		showInformationDialog("导出完成", fmt.Sprintf("已导出 %d 张表、%d 行数据\n%s", result.Tables, result.Rows, path), n.owner.Window)
	})
}

func databaseExportProgressText(p databaseexport.Progress) string {
	text := fmt.Sprintf("阶段：%s\n已完成：%d / %d 张表\n已导出：%d 行 · %.2f MiB", p.Phase, p.CompletedTables, p.Total, p.Rows, float64(p.Bytes)/(1<<20))
	if p.Table != "" {
		text += fmt.Sprintf("\n当前表：%s\n当前表已导出：%d 行", p.Table, p.TableRows)
	}
	return text
}
