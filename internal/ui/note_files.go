package ui

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode/utf8"

	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/filedialog"
	"github.com/ealink1/super-link/internal/notefile"
)

func (n *noteWorkspace) importMarkdown() {
	if !n.loaded {
		return
	}
	n.owner.jobs.run(func(ctx context.Context) (any, error) {
		path, err := filedialog.SelectMarkdown(ctx)
		if err != nil || path == "" {
			return nil, err
		}
		reader, err := os.Open(path)
		if err != nil {
			return nil, err
		}
		defer reader.Close()
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		raw, err := io.ReadAll(io.LimitReader(reader, domain.MaxNoteBody+1))
		if err != nil {
			return nil, err
		}
		if len(raw) > domain.MaxNoteBody || !utf8.Valid(raw) {
			return nil, errors.New("请选择不超过 256 KiB 的 UTF-8 Markdown 文件")
		}
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		return struct{ name, body string }{strings.TrimSuffix(filepath.Base(path), filepath.Ext(path)), string(raw)}, nil
	}, func(value any, err error) {
		if err != nil {
			n.status.SetText(err.Error())
			return
		}
		if value == nil {
			return
		}
		imported := value.(struct{ name, body string })
		previous := len(n.book.Notes)
		n.newNote()
		if len(n.book.Notes) != previous+1 || n.current() == nil {
			return
		}
		n.title.SetText(truncateShellTitle(imported.name, 200))
		n.editor.SetText(imported.body)
		n.save()
	})
}

func (n *noteWorkspace) exportMarkdown() {
	note := n.current()
	if note == nil {
		return
	}
	body, filename := note.Body, noteFilename(note.Title)
	n.owner.chooseLocalPath("选择 Markdown 导出目录", true, nil, func(directory string) {
		name := widget.NewEntry()
		name.SetText(filename)
		modal := dialog.NewForm("导出 Markdown", "导出", "取消", []*widget.FormItem{widget.NewFormItem("文件名", name)}, func(ok bool) {
			if ok {
				n.saveMarkdownFile(filepath.Join(directory, noteFilename(strings.TrimSuffix(name.Text, ".md"))), body, false)
			}
		}, n.owner.Window)
		modal.Show()
	})
}
func (n *noteWorkspace) saveMarkdownFile(path, body string, overwrite bool) {
	n.owner.jobs.run(func(ctx context.Context) (any, error) { return nil, notefile.Save(ctx, path, body, overwrite) }, func(_ any, err error) {
		if errors.Is(err, os.ErrExist) && !overwrite {
			dialog.ShowConfirm("覆盖 Markdown 文件", "替换「"+filepath.Base(path)+"」？", func(ok bool) {
				if ok {
					n.saveMarkdownFile(path, body, true)
				}
			}, n.owner.Window)
			return
		}
		if err != nil {
			n.status.SetText("导出失败，目标文件保留：" + err.Error())
			return
		}
		n.status.SetText("Markdown 已导出")
	})
}
func noteFilename(title string) string {
	title = strings.TrimSpace(title)
	title = strings.Map(func(r rune) rune {
		if r < 32 || strings.ContainsRune(`/\:*?"<>|`, r) {
			return '_'
		}
		return r
	}, title)
	title = strings.Trim(title, ". ")
	if title == "" {
		title = "note"
	}
	return truncateShellTitle(title, 80) + ".md"
}
