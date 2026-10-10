package ui

import (
	"context"
	"errors"
	"fmt"
	"image/color"
	"os"
	"path"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/infra/filedialog"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func (f *shellFiles) selectedFile() (transport.File, string, bool) {
	if f.selected < 0 || f.selected >= len(f.files) {
		return transport.File{}, "", false
	}
	file := f.files[f.selected]
	if !validShellFileName(file.Name) {
		return file, "", false
	}
	return file, path.Join(f.directory.Text, file.Name), true
}
func validShellFileName(name string) bool {
	return name != "" && name != "." && name != ".." && len(name) <= 255 && !strings.ContainsAny(name, "/\\\x00")
}
func (f *shellFiles) showMenu(position fyne.Position) {
	file, source, ok := f.selectedFile()
	if !ok {
		return
	}
	var popup *widget.PopUp
	items := []fyne.CanvasObject{}
	add := func(label, icon string, enabled bool, destructive bool, action func()) {
		button := shellButton(label, icon, false, func() { popup.Hide(); action() })
		button.Alignment = widget.ButtonAlignLeading
		button.SetIcon(shellFileActionIcon(icon, destructive))
		if !enabled {
			button.Disable()
		}
		shade := color.Color(shellTextColor)
		if destructive {
			shade = color.NRGBA{255, 88, 119, 255}
		}
		item := container.NewThemeOverride(button, shellFileMenuTheme{monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 13}, shade: shade}})
		items = append(items, shellFixed(item, 176, 33))
	}
	ready := !f.busy && !f.pane.ended
	if file.Directory {
		add("打开文件夹", "folder", ready, false, f.enter)
	}
	add("复制文件名", "copy", true, false, func() { f.pane.workspace.owner.Window.Clipboard().SetContent(file.Name) })
	add("复制路径", "link", true, false, func() { f.pane.workspace.owner.Window.Clipboard().SetContent(source) })
	add("用本地程序打开", "external-link", ready && !file.Directory, false, f.openLocal)
	add("下载", "download", ready && !file.Directory, false, f.download)
	add("重命名…", "pencil", ready, false, func() { f.rename(file, source) })
	add("修改权限…", "shield", ready, false, func() { f.permissions(file, source) })
	add("删除", "trash-2", ready, true, func() { f.remove(file, source) })
	body := container.NewThemeOverride(shellPanel(shellVBox(items...), monitorSurfaceColor, 9, 4), newShellTheme())
	popup = widget.NewPopUp(body, f.pane.workspace.owner.Window.Canvas())
	popup.ShowAtPosition(position)
}

func (f *shellFiles) rename(file transport.File, source string) {
	entry := widget.NewEntry()
	entry.SetText(file.Name)
	entry.Validator = func(name string) error {
		if !validShellFileName(name) {
			return errors.New("请输入有效文件名，不能包含路径分隔符")
		}
		return nil
	}
	d := dialog.NewForm("重命名", "保存", "取消", []*widget.FormItem{widget.NewFormItem("名称", entry)}, func(ok bool) {
		if !ok || entry.Text == file.Name {
			return
		}
		if err := entry.Validate(); err != nil {
			f.status.SetText(err.Error())
			return
		}
		destination := path.Join(path.Dir(source), entry.Text)
		f.operation("重命名", func(ctx context.Context) error { return f.remote.RenameFile(ctx, source, destination) })
	}, f.pane.workspace.owner.Window)
	d.Resize(fyne.NewSize(340, 150))
	d.Show()
}
func shellFilePermissions(raw string) string {
	var bits uint64
	if len(raw) >= 10 {
		for i, c := range raw[1:10] {
			if c != '-' {
				bits |= 1 << uint(8-i)
			}
		}
	}
	return fmt.Sprintf("%03o", bits)
}
func parseShellPermissions(raw string) (os.FileMode, error) {
	if len(raw) != 3 || strings.Trim(raw, "01234567") != "" {
		return 0, errors.New("请输入三位八进制权限，例如 644 或 755")
	}
	bits, err := strconv.ParseUint(raw, 8, 9)
	return os.FileMode(bits), err
}
func (f *shellFiles) permissions(file transport.File, source string) {
	entry := widget.NewEntry()
	entry.SetText(shellFilePermissions(file.Mode))
	entry.Validator = func(raw string) error { _, err := parseShellPermissions(raw); return err }
	dialog.NewForm("修改权限 · "+file.Name, "保存", "取消", []*widget.FormItem{widget.NewFormItem("权限", entry)}, func(ok bool) {
		if !ok {
			return
		}
		mode, err := parseShellPermissions(entry.Text)
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		f.operation("修改权限", func(ctx context.Context) error { return f.remote.SetFilePermissions(ctx, source, mode) })
	}, f.pane.workspace.owner.Window).Show()
}
func (f *shellFiles) remove(file transport.File, source string) {
	message := "删除 “" + file.Name + "”？此操作无法撤销。"
	if file.Directory {
		message += "仅支持删除空目录。"
	}
	newConfirmDialog("删除", message, func(ok bool) {
		if ok {
			f.operation("删除", func(ctx context.Context) error { return f.remote.RemoveFile(ctx, source) })
		}
	}, f.pane.workspace.owner.Window).Show()
}
func (f *shellFiles) operation(label string, work func(context.Context) error) {
	if f.busy || f.pane.closed || f.pane.ended {
		return
	}
	f.busy = true
	f.status.SetText(label + "…")
	f.pane.workspace.owner.jobs.run(func(context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(f.pane.ctx, 30*time.Second)
		defer cancel()
		return nil, work(ctx)
	}, func(_ any, err error) {
		f.busy = false
		if f.pane.closed {
			return
		}
		if err != nil {
			f.status.SetText(label + "失败：" + err.Error())
			return
		}
		f.refresh()
	})
}
func (f *shellFiles) openLocal() {
	file, source, ok := f.selectedFile()
	if !ok || file.Directory || f.busy || f.pane.ended {
		return
	}
	var directory string
	f.transfer("下载", file.Name, file.Size, func(ctx context.Context, progress func(int64)) error {
		var err error
		directory, err = os.MkdirTemp("", "SuperLink-open-")
		if err != nil {
			return err
		}
		local := filepath.Join(directory, file.Name)
		if err = f.remote.Download(ctx, source, local, progress); err != nil {
			return err
		}
		return filedialog.OpenLocal(ctx, local)
	}, func(err error) {
		if directory == "" {
			return
		}
		if err != nil || f.pane.closed {
			go os.RemoveAll(directory)
		} else {
			f.localDownloads = append(f.localDownloads, directory)
		}
	})
}

type shellFileMenuTheme struct{ monitorTintTheme }

func (t shellFileMenuTheme) Font(style fyne.TextStyle) fyne.Resource {
	style.Bold = false
	return t.monitorTintTheme.Font(style)
}
func shellFileActionIcon(name string, destructive bool) fyne.Resource {
	paths := map[string]string{
		"copy":          `<rect x="8" y="8" width="12" height="13" rx="1"/><path d="M5 16H3V3h13v2"/>`,
		"link":          `<path d="m9 15 6-6m-7 3-3 3a4 4 0 0 0 6 6l3-3m-2-7 3-3a4 4 0 0 1 6 6l-3 3"/>`,
		"external-link": `<path d="M14 3h7v7m0-7-11 11M9 5H3v16h16v-6"/>`,
		"download":      `<path d="M12 3v12m-5-5 5 5 5-5M3 15v6h18v-6"/>`,
		"pencil":        `<path d="m3 21 2-7L17 2l5 5L10 19l-7 2Zm12-17 5 5"/>`,
		"shield":        `<path d="m12 2 9 4v6c0 6-9 10-9 10S3 18 3 12V6l9-4Z"/>`,
		"trash-2":       `<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 9v9m4-9v9"/>`,
		"folder":        `<path d="M3 20V4h6l2 3h10v13H3Z"/>`,
	}
	shade := color.Color(monitorMutedColor)
	if destructive {
		shade = color.NRGBA{255, 88, 119, 255}
	}
	c := color.NRGBAModel.Convert(resolveShellColor(shade)).(color.NRGBA)
	return fyne.NewStaticResource("file-action-"+name+".svg", []byte(fmt.Sprintf(`<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#%02x%02x%02x" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">%s</svg>`, c.R, c.G, c.B, paths[name])))
}
