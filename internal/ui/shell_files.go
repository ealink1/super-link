package ui

import (
	"context"
	"errors"
	"fmt"
	"path"
	"path/filepath"
	"sort"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellFiles struct {
	pane         *shellPane
	remote       *transport.Remote
	directory    *widget.Entry
	list         *widget.List
	search       *widget.Entry
	shown        []int
	files        []transport.File
	selected     int
	busy         bool
	status       *widget.Label
	content      *fyne.Container
	cancel       context.CancelFunc
	cancelButton *shellAlignedButton
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
	dialog.ShowFileOpen(func(reader fyne.URIReadCloser, err error) {
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		if reader == nil {
			return
		}
		source := reader.URI().Path()
		reader.Close()
		destination := path.Join(f.directory.Text, filepath.Base(source))
		f.transfer(func(ctx context.Context, progress func(int64)) error {
			return f.remote.Upload(ctx, source, destination, progress)
		})
	}, f.pane.workspace.owner.Window)
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
	dialog.ShowFolderOpen(func(folder fyne.ListableURI, err error) {
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		if folder == nil {
			return
		}
		destination := filepath.Join(folder.Path(), file.Name)
		f.transfer(func(ctx context.Context, progress func(int64)) error {
			return f.remote.Download(ctx, source, destination, progress)
		})
	}, f.pane.workspace.owner.Window)
}
func (f *shellFiles) transfer(work func(context.Context, func(int64)) error) {
	if f.busy || f.pane.closed {
		return
	}
	f.busy = true
	f.status.SetText("正在传输…已有同名文件时会停止。")
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
				if !f.pane.closed && f.busy {
					f.status.SetText(fmt.Sprintf("已传输 %.1f MiB", float64(size)/(1<<20)))
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
		if f.pane.closed {
			return
		}
		if err != nil {
			if errors.Is(err, context.Canceled) {
				f.status.SetText("传输已取消：" + err.Error())
				return
			}
			f.status.SetText("传输失败：" + err.Error())
			return
		}
		f.refresh()
	})
}
