package ui

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/infra/filedialog"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellFiles struct {
	pane           *shellPane
	remote         *transport.Remote
	directory      *widget.Entry
	list           *widget.List
	search         *widget.Entry
	shown          []int
	files          []transport.File
	selected       int
	busy           bool
	status         *widget.Label
	content        *fyne.Container
	cancel         context.CancelFunc
	cancelButton   *shellAlignedButton
	progress       *widget.ProgressBar
	transferLabel  *widget.Label
	transferBar    *fyne.Container
	localDownloads []string
}

func (p *shellPane) showFiles() {
	remote, ok := p.session.(*transport.Remote)
	if !ok || p.ended {
		p.status.SetText("SFTP 需要已连接的 SSH 会话")
		return
	}
	if p.filePane == nil {
		p.filePane = newShellFiles(p, remote)
	}
	p.showAuxiliary("files", "文件管理 · SFTP", p.filePane.content)
}
func newShellFiles(p *shellPane, remote *transport.Remote) *shellFiles {
	f := &shellFiles{pane: p, remote: remote, selected: -1, directory: widget.NewEntry(), status: widget.NewLabel("SFTP")}
	f.status.Wrapping = fyne.TextWrapWord
	f.directory.SetText(".")
	f.content = newShellFilesView(f)
	f.refresh()
	return f
}
func (f *shellFiles) refresh() {
	if f.busy || f.pane.closed || f.pane.ended {
		return
	}
	f.busy = true
	f.status.SetText("读取目录…")
	directory := f.directory.Text
	f.pane.workspace.owner.jobs.run(func(context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(f.pane.ctx, 30*time.Second)
		defer cancel()
		return f.remote.Files(ctx, directory)
	}, func(value any, err error) {
		f.busy = false
		if f.pane.closed {
			return
		}
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		f.files = value.([]transport.File)
		sort.Slice(f.files, func(i, j int) bool {
			a, b := f.files[i], f.files[j]
			if a.Directory != b.Directory {
				return a.Directory
			}
			return a.Name < b.Name
		})
		f.filterFiles()
		if f.pane.monitorPane != nil {
			f.pane.monitorPane.setDirectorySummary(directory, f.files)
		}
		f.status.SetText(fmt.Sprintf("%d 项", len(f.files)))
	})
}
func (f *shellFiles) enter() {
	if f.selected >= 0 && f.selected < len(f.files) && f.files[f.selected].Directory {
		f.directory.SetText(path.Join(f.directory.Text, f.files[f.selected].Name))
		f.refresh()
	}
}
func (f *shellFiles) upload() {
	if f.busy || f.pane.ended {
		return
	}
	directory := f.directory.Text
	f.pickLocalPath("选择要上传的文件", false, func(source string) {
		destination := path.Join(directory, filepath.Base(source))
		f.transfer("上传", filepath.Base(source), 0, func(ctx context.Context, progress func(int64)) error {
			return f.remote.Upload(ctx, source, destination, progress)
		}, nil)
	})
}
func (f *shellFiles) download() {
	if f.busy || f.pane.ended || f.selected < 0 || f.selected >= len(f.files) || f.files[f.selected].Directory {
		return
	}
	file := f.files[f.selected]
	if filepath.Base(file.Name) != file.Name || file.Name == "." || file.Name == ".." {
		f.status.SetText("远程文件名称无效")
		return
	}
	source := path.Join(f.directory.Text, file.Name)
	f.pickLocalPath("选择下载保存目录", true, func(folder string) {
		destination := filepath.Join(folder, file.Name)
		f.transfer("下载", file.Name, file.Size, func(ctx context.Context, progress func(int64)) error {
			return f.remote.Download(ctx, source, destination, progress)
		}, nil)
	})
}
func (f *shellFiles) transfer(direction, name string, total int64, work func(context.Context, func(int64)) error, complete func(error)) {
	if f.busy || f.pane.closed {
		return
	}
	f.busy = true
	f.status.SetText("已有同名文件时会停止。")
	f.setTransferProgress(direction, name, 0, total)
	f.transferBar.Show()
	ctx, cancel := context.WithCancel(f.pane.ctx)
	f.cancel = cancel
	f.cancelButton.Enable()
	f.pane.workspace.owner.jobs.run(func(context.Context) (any, error) {
		defer cancel()
		var last time.Time
		progress := func(size int64) {
			if time.Since(last) < 250*time.Millisecond {
				return
			}
			last = time.Now()
			f.pane.workspace.owner.jobs.dispatch(func() {
				if !f.pane.closed && f.busy && ctx.Err() == nil {
					f.setTransferProgress(direction, name, size, total)
				}
			})
		}
		err := work(ctx, progress)
		if err != nil && ctx.Err() != nil {
			err = errors.Join(err, ctx.Err())
		}
		return nil, err
	}, func(_ any, err error) {
		f.busy = false
		f.cancel = nil
		f.cancelButton.Disable()
		if complete != nil {
			complete(err)
		}
		if f.pane.closed {
			return
		}
		if err != nil {
			if errors.Is(err, context.Canceled) {
				f.transferLabel.SetText(direction + "已取消 · " + name)
				f.status.SetText("传输已取消：" + err.Error())
				return
			}
			f.transferLabel.SetText(direction + "失败 · " + name)
			f.status.SetText("传输失败：" + err.Error())
			return
		}
		f.setTransferProgress(direction, name, total, total)
		f.transferLabel.SetText(direction + "完成 · " + name)
		f.status.SetText(direction + "完成")
		if direction == "上传" {
			f.refresh()
		}
	})
}

// Native pickers can wait for user input; keep them off the Fyne goroutine.
func (f *shellFiles) pickLocalPath(title string, directory bool, selected func(string)) {
	if f.busy || f.pane.closed || f.pane.ended {
		return
	}
	f.busy = true
	f.pane.workspace.owner.jobs.run(func(jobCtx context.Context) (any, error) {
		ctx, cancel := context.WithCancel(f.pane.ctx)
		defer cancel()
		stop := context.AfterFunc(jobCtx, cancel)
		defer stop()
		return filedialog.Select(ctx, title, directory)
	}, func(value any, err error) {
		f.busy = false
		if f.pane.closed || f.pane.ended {
			return
		}
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		if file := value.(string); file != "" {
			selected(file)
		}
	})
}

func (f *shellFiles) setTransferProgress(direction, name string, size, total int64) {
	fraction := float64(0)
	if total > 0 {
		fraction = min(1, max(0, float64(size)/float64(total)))
	}
	f.progress.SetValue(fraction)
	text := direction + "  " + name + "  " + shellFileSize(size)
	if total > 0 {
		text = fmt.Sprintf("%s  %s  %.0f%% · %s / %s", direction, name, fraction*100, shellFileSize(size), shellFileSize(total))
	}
	f.transferLabel.SetText(text)
}
func (f *shellFiles) cleanupLocalDownloads() {
	directories := append([]string(nil), f.localDownloads...)
	f.localDownloads = nil
	go func() {
		for _, directory := range directories {
			os.RemoveAll(directory)
		}
	}()
}
