// Package filedialog selects local paths using the operating system's dialogs.
package filedialog

import (
	"context"
	"errors"
	"fmt"
	"os"

	"github.com/ncruces/zenity"
)

func Select(ctx context.Context, title string, directory bool) (string, error) {
	return SelectFiltered(ctx, title, directory, nil)
}
func SelectMarkdown(ctx context.Context) (string, error) {
	return SelectFiltered(ctx, "导入 Markdown", false, []string{"md", "markdown", "txt"})
}
func SelectFiltered(ctx context.Context, title string, directory bool, extensions []string) (string, error) {
	var filters []zenity.Option
	if len(extensions) > 0 {
		patterns := make([]string, 0, len(extensions))
		for _, extension := range extensions {
			patterns = append(patterns, "*."+extension)
		}
		filters = append(filters, zenity.FileFilter{Name: title, Patterns: patterns, CaseFold: true})
	}
	return selectPath(ctx, title, directory, platformPicker(ctx, title, directory, extensions), filters...)
}
func selectPath(ctx context.Context, title string, directory bool, picker func(...zenity.Option) (string, error), filters ...zenity.Option) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	options := []zenity.Option{zenity.Title(title), zenity.Context(ctx)}
	options = append(options, filters...)
	if directory {
		options = append(options, zenity.Directory())
	}
	selected, err := picker(options...)
	if errors.Is(err, zenity.ErrCanceled) {
		return "", nil
	}
	if err != nil {
		return "", fmt.Errorf("打开系统文件选择器失败：%w", err)
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if selected == "" {
		return "", nil
	}
	info, err := os.Stat(selected)
	if err != nil {
		return "", err
	}
	if directory && !info.IsDir() {
		return "", errors.New("请选择目录")
	}
	if !directory && !info.Mode().IsRegular() {
		return "", errors.New("请选择普通文件")
	}
	return selected, nil
}
