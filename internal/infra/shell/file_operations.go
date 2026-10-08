package shell

import (
	"context"
	"errors"
	"os"
	"path"
	"strings"
)

func validRemoteFilePath(filename string) error {
	if filename == "" || len(filename) > 4096 || strings.ContainsRune(filename, 0) || path.Base(filename) == "." || path.Clean(filename) == "/" || path.Base(filename) == ".." {
		return errors.New("远程文件路径无效")
	}
	return nil
}

// RenameFile uses the SFTP v3 rename operation, which refuses existing targets.
func (r *Remote) RenameFile(ctx context.Context, source, destination string) error {
	if err := validRemoteFilePath(source); err != nil {
		return err
	}
	if err := validRemoteFilePath(destination); err != nil {
		return err
	}
	client, closeClient, err := r.fileClient(ctx)
	if err != nil {
		return err
	}
	defer closeClient()
	if _, err := client.Lstat(destination); err == nil {
		return errors.New("目标名称已存在")
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return client.Rename(source, destination)
}

func (r *Remote) SetFilePermissions(ctx context.Context, filename string, mode os.FileMode) error {
	if err := validRemoteFilePath(filename); err != nil {
		return err
	}
	if mode & ^os.FileMode(0777) != 0 {
		return errors.New("权限必须为 000 至 777")
	}
	client, closeClient, err := r.fileClient(ctx)
	if err != nil {
		return err
	}
	defer closeClient()
	return client.Chmod(filename, mode)
}

// RemoveFile never recursively removes directories or follows symlinks.
func (r *Remote) RemoveFile(ctx context.Context, filename string) error {
	if err := validRemoteFilePath(filename); err != nil {
		return err
	}
	client, closeClient, err := r.fileClient(ctx)
	if err != nil {
		return err
	}
	defer closeClient()
	info, err := client.Lstat(filename)
	if err != nil {
		return err
	}
	if info.IsDir() {
		return client.RemoveDirectory(filename)
	}
	return client.Remove(filename)
}
