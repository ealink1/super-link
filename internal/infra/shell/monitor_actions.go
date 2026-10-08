package shell

import (
	"context"
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"
)

type LargeFile struct {
	Path string
	Size uint64
}

func largeFilesCommand(paths []string, minimum uint64, count int) (string, error) {
	if len(paths) == 0 || len(paths) > 8 || minimum < 1<<20 || minimum > 1<<50 || count < 1 || count > 100 {
		return "", errors.New("扫描参数无效")
	}
	quoted := make([]string, len(paths))
	for i, p := range paths {
		if !strings.HasPrefix(p, "/") || strings.ContainsAny(p, "\x00\r\n") || len(p) > 4096 {
			return "", errors.New("请输入绝对目录路径")
		}
		quoted[i] = "'" + strings.ReplaceAll(p, "'", "'\"'\"'") + "'"
	}
	command := fmt.Sprintf("LC_ALL=C find %s -xdev -type f -size +%dc -printf '%%s\\t%%p\\0' 2>/dev/null | sort -z -nr | head -z -n %d", strings.Join(quoted, " "), minimum-1, count)
	return "timeout 25s sh -c " + "'" + strings.ReplaceAll(command, "'", "'\"'\"'") + "'", nil
}
func (r *Remote) LargeFiles(ctx context.Context, paths []string, minimum uint64, count int) ([]LargeFile, error) {
	command, err := largeFilesCommand(paths, minimum, count)
	if err != nil {
		return nil, err
	}
	raw, err := r.monitorOperation(ctx, command, 30*time.Second)
	if err != nil {
		return nil, err
	}
	var files []LargeFile
	for _, line := range strings.Split(raw, "\x00") {
		v := strings.SplitN(line, "\t", 2)
		if len(v) == 2 && len(files) < count {
			files = append(files, LargeFile{v[1], number(v[0])})
		}
	}
	return files, nil
}
func stopProcessCommand(p ProcessMetric) (string, error) {
	if p.PID <= 1 || p.Start == 0 {
		return "", errors.New("无法安全识别此进程，请刷新后重试")
	}
	// Check kernel start ticks immediately before SIGTERM to prevent PID reuse.
	return fmt.Sprintf("test \"$(awk '{sub(/^.*\\) /,\"\");print $20}' /proc/%d/stat 2>/dev/null)\" = %s && kill -TERM -- %d", p.PID, strconv.FormatUint(p.Start, 10), p.PID), nil
}
func (r *Remote) StopProcess(ctx context.Context, p ProcessMetric) error {
	command, err := stopProcessCommand(p)
	if err != nil {
		return err
	}
	_, err = r.monitorOperation(ctx, command, 8*time.Second)
	return err
}
func (r *Remote) monitorOperation(ctx context.Context, command string, timeout time.Duration) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	s, closeSession, err := r.auxiliarySession(ctx)
	if err != nil {
		return "", err
	}
	defer closeSession()
	output := &boundedOutput{limit: 256 << 10}
	s.Stdout, s.Stderr = output, io.Discard
	if err = s.Run(command); err != nil {
		if ctx.Err() != nil {
			return "", ctx.Err()
		}
		return "", err
	}
	return output.String(), nil
}
