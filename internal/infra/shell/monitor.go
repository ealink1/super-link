package shell

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"
)

// Metrics is a bounded Linux server health snapshot.
type Metrics struct {
	System, Load, Uptime, Disk string
	MemoryTotal, MemoryUsed    uint64
	Details                    MonitorDetails
}

const monitorCommand = `LC_ALL=C; export LC_ALL; uname -s; printf '\n__LOAD__\n'; cat /proc/loadavg 2>/dev/null; printf '\n__MEM__\n'; cat /proc/meminfo 2>/dev/null; printf '\n__UPTIME__\n'; cat /proc/uptime 2>/dev/null; printf '\n__DISK__\n'; df -P / 2>/dev/null`

// Monitor executes a fixed read-only probe, without interpolating user input.
func (r *Remote) Monitor(ctx context.Context) (Metrics, error) {
	ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	session, closeSession, err := r.auxiliarySession(ctx)
	if err != nil {
		return Metrics{}, err
	}
	defer closeSession()
	output := &boundedOutput{limit: 256 << 10}
	session.Stdout, session.Stderr = output, io.Discard
	if err := session.Run(monitorCommand + detailedMonitorCommand); err != nil {
		return Metrics{}, err
	}
	return parseMetrics(output.String())
}

type boundedOutput struct {
	bytes.Buffer
	limit int
}

func (b *boundedOutput) Write(p []byte) (int, error) {
	if b.Len()+len(p) > b.limit {
		return 0, errors.New("监控输出超过预算")
	}
	return b.Buffer.Write(p)
}

func parseMetrics(raw string) (Metrics, error) {
	parts := strings.SplitN(raw, "__DETAILS__", 2)
	sections := strings.Split(parts[0], "__")
	if len(sections) != 9 {
		return Metrics{}, errors.New("服务器监控响应格式无效")
	}
	m := Metrics{System: strings.TrimSpace(sections[0]), Load: strings.TrimSpace(sections[2]), Uptime: strings.TrimSpace(sections[6]), Disk: strings.TrimSpace(sections[8])}
	if m.System != "Linux" {
		return m, fmt.Errorf("当前监控探针支持 Linux，此主机为 %s", m.System)
	}
	var available uint64
	var hasAvailable bool
	for _, line := range strings.Split(sections[4], "\n") {
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		value, err := strconv.ParseUint(fields[1], 10, 64)
		if err != nil || value > ^uint64(0)/1024 {
			continue
		}
		switch fields[0] {
		case "MemTotal:":
			m.MemoryTotal = value * 1024
		case "MemAvailable:":
			available = value * 1024
			hasAvailable = true
		}
	}
	if m.MemoryTotal == 0 || !hasAvailable || available > m.MemoryTotal {
		return m, errors.New("服务器内存指标无效")
	}
	m.MemoryUsed = m.MemoryTotal - available
	if len(parts) == 2 {
		m.Details = parseMonitorDetails(parts[1], sections[4])
	}
	return m, nil
}
