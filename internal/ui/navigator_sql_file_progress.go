package ui

import (
	"context"
	"fmt"
	"path/filepath"
	"sync"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
)

type sqlFileDialog struct {
	navigator         *navigator
	profile           domain.Profile
	scope, path       string
	modal             *dialog.CustomDialog
	label             *widget.Label
	bar               *widget.ProgressBar
	confirm, stop     *widget.Button
	prepared          *application.PreparedSQLFile
	cancel            context.CancelFunc
	active, executing bool
	running           bool
	generation        uint64
}

func (n *navigator) showSQLFileDialog(p domain.Profile, scope, path string) {
	d := &sqlFileDialog{navigator: n, profile: p, scope: scope, path: path, active: true, label: widget.NewLabel("正在统计 SQL 语句…"), bar: widget.NewProgressBar()}
	d.label.Wrapping = fyne.TextWrapWord
	d.confirm = widget.NewButton("确认执行", d.execute)
	d.confirm.Importance = widget.HighImportance
	d.confirm.Disable()
	d.stop = widget.NewButton("停止", func() {
		if d.cancel != nil {
			d.cancel()
		}
		d.confirm.Disable()
		d.stop.Disable()
	})
	content := container.NewBorder(nil, container.NewHBox(d.confirm, d.stop), nil, nil, container.NewVBox(d.label, d.bar))
	d.modal = dialog.NewCustom("执行 SQL 文件", "关闭", content, n.owner.Window)
	d.modal.Resize(fyne.NewSize(560, 300))
	d.modal.SetOnClosed(func() {
		d.active = false
		if d.cancel != nil {
			d.cancel()
		}
		if d.prepared != nil {
			d.prepared.Close()
		}
	})
	d.modal.Show()
	d.run(false)
}

func (d *sqlFileDialog) execute() {
	if !d.active || d.executing || d.prepared == nil {
		return
	}
	d.executing = true
	d.confirm.Disable()
	d.stop.Enable()
	d.run(true)
}

// Preparation and execution share one dialog and one throttled progress view.
func (d *sqlFileDialog) run(execute bool) {
	n := d.navigator
	d.generation++
	generation := d.generation
	d.running = true
	n.owner.jobs.runWithCancel(&d.cancel, func(ctx context.Context) (any, error) {
		started := time.Now()
		var mu sync.Mutex
		latest := application.SQLFileProgress{Phase: "统计语句"}
		executionStarted := time.Time{}
		render := func() {
			text, fraction := sqlFileProgressText(d.path, latest, time.Since(started), executionStarted)
			n.owner.jobs.dispatch(func() {
				if d.active && d.running && d.generation == generation && !n.owner.jobs.closing.Load() {
					d.label.SetText(text)
					d.bar.SetValue(fraction)
				}
			})
		}
		update := func(progress application.SQLFileProgress) {
			mu.Lock()
			defer mu.Unlock()
			latest = progress
			if progress.Phase == "执行 SQL" && executionStarted.IsZero() {
				executionStarted = time.Now()
			}
			render()
		}
		done := make(chan struct{})
		defer close(done)
		go func() {
			ticker := time.NewTicker(time.Second)
			defer ticker.Stop()
			for {
				select {
				case <-done:
					return
				case <-ctx.Done():
					return
				case <-ticker.C:
					mu.Lock()
					render()
					mu.Unlock()
				}
			}
		}()
		if execute {
			return d.prepared.Execute(ctx, true, update)
		}
		return n.owner.Engine.PrepareSQLFile(ctx, d.profile.ID, d.scope, d.path, d.profile.Revision, update)
	}, func(value any, err error) {
		d.running = false
		if !d.active {
			if prepared, ok := value.(*application.PreparedSQLFile); ok && prepared != nil {
				prepared.Close()
			}
			return
		}
		if err != nil {
			d.confirm.Disable()
			d.stop.Disable()
			if execute {
				completed, _ := value.(int64)
				d.label.SetText(fmt.Sprintf("已执行 %d 条 SQL\n已停止：%v", completed, err))
			} else {
				d.label.SetText("已停止：" + err.Error())
			}
			return
		}
		if !execute {
			d.prepared = value.(*application.PreparedSQLFile)
			info := d.prepared.Info()
			access := "只读查询"
			if info.Write {
				access = "包含写入操作，将修改数据库；停止不会回滚已执行语句。"
			}
			d.label.SetText(fmt.Sprintf("文件：%s\n连接：%s / %s\n统计语句、摘要核对均已完成。\n共 %d 条 SQL · %.1f MiB\n%s\n\n点击“确认执行”后开始。", filepath.Base(d.path), d.profile.Name, d.scope, info.Statements, float64(info.Bytes)/(1<<20), access))
			d.bar.SetValue(1)
			d.confirm.Enable()
			d.stop.Disable()
			return
		}
		completed, _ := value.(int64)
		d.stop.Disable()
		d.label.SetText(fmt.Sprintf("SQL 文件执行完成\n文件：%s\n已执行 %d 条 SQL", filepath.Base(d.path), completed))
		d.bar.SetValue(1)
		for _, node := range n.nodes {
			if node.kind == "database" && node.profileID == d.profile.ID && node.scope == d.scope {
				n.selected = node.id
				n.refresh()
				break
			}
		}
	})
}

func sqlFileProgressText(path string, progress application.SQLFileProgress, elapsed time.Duration, executionStarted time.Time) (string, float64) {
	fraction := 0.0
	detail := fmt.Sprintf("已核对：%d 条 SQL", progress.Checked)
	if progress.Phase == "统计语句" {
		detail = fmt.Sprintf("已统计：%d 条 SQL", progress.Checked)
	}
	speed := float64(progress.Bytes) / (1 << 20) / max(elapsed.Seconds(), 0.001)
	rate := fmt.Sprintf("读取速度：%.1f MiB/s", speed)
	if progress.TotalBytes > 0 {
		fraction = float64(progress.Bytes) / float64(progress.TotalBytes)
	}
	if progress.Phase != "核对文件" && progress.Phase != "统计语句" && progress.Phase != "核对摘要" {
		fraction = 0
		if progress.Total > 0 {
			fraction = float64(progress.Completed) / float64(progress.Total)
		}
		detail = fmt.Sprintf("已执行：%d / %d 条 SQL", progress.Completed, progress.Total)
		if progress.Current > 0 {
			detail += fmt.Sprintf(" · 正在执行第 %d 条", progress.Current)
		}
		rate = "等待数据库响应"
		if !executionStarted.IsZero() {
			rate = fmt.Sprintf("执行速度：%.1f 条/s", float64(progress.Completed)/max(time.Since(executionStarted).Seconds(), 0.001))
		}
	}
	return fmt.Sprintf("文件：%s\n阶段：%s\n%s\n已读取：%.1f / %.1f MiB\n耗时：%s · %s", filepath.Base(path), progress.Phase, detail, float64(progress.Bytes)/(1<<20), float64(progress.TotalBytes)/(1<<20), elapsed.Round(time.Second), rate), min(1, max(0, fraction))
}
