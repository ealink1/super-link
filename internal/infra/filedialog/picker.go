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
	return selectPath(ctx, title, directory, zenity.SelectFile)
}
func selectPath(ctx context.Context, title string, directory bool, picker func(...zenity.Option) (string, error)) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	options := []zenity.Option{zenity.Title(title), zenity.Context(ctx)}
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
