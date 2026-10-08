package shell

import (
	"strconv"
	"strings"
	"time"
)

type CPUCounter struct {
	Name                            string
	User, System, Idle, Wait, Total uint64
}
type NetworkCounter struct {
	Name           string
	Received, Sent uint64
}
type DiskCounter struct {
	Name          string
	Reads, Writes uint64
}
type Partition struct {
	Device, Mount string
	Total, Used   uint64
	Percent       float64
}
type ProcessMetric struct {
	PID         int
	User, Name  string
	CPU, Memory float64
	RSS         uint64
	Start       uint64
}
type Listener struct{ Protocol, Address string }
type GPU struct {
	Name                     string
	Used, Total              uint64
	Utilization, Temperature float64
}
type MonitorDetails struct {
	Sampled                                                                     time.Time
	Connections, TCPConnections, TimeWait, UDPConnections, Handles, HandleLimit uint64
	Listeners                                                                   []Listener
	Addresses                                                                   map[string]string
	Kernel                                                                      string
	CPUs                                                                        []CPUCounter
	Networks                                                                    []NetworkCounter
	Disks                                                                       []DiskCounter
	Partitions                                                                  []Partition
	Processes                                                                   []ProcessMetric
	GPUs                                                                        []GPU
	MemoryFree, MemoryAvailable, Cached, Buffers, SwapTotal, SwapUsed           uint64
	Temperature                                                                 float64
}

// Optional probes use independent tagged sections, so unavailable sensors never
// invalidate the basic Linux snapshot. Process arguments are intentionally absent.
const detailedMonitorCommand = `; printf '\n__DETAILS__\n'; printf '@KERNEL\n'; uname -snr; printf '@CPU\n'; cat /proc/stat; printf '@NET\n'; cat /proc/net/dev; printf '@IO\n'; cat /proc/diskstats; printf '@PART\n'; df -Pk -x tmpfs -x devtmpfs -x overlay -x squashfs; printf '@PROC\n'; { ps -eo pid=,user=,pcpu=,pmem=,rss=,comm= --sort=-pcpu | head -20; ps -eo pid=,user=,pcpu=,pmem=,rss=,comm= --sort=-rss | head -20; } | awk '!seen[$1]++' | while read pid user cpu mem rss name; do start=$(awk '{sub(/^.*\) /,"");print $20}' /proc/$pid/stat 2>/dev/null); printf '%s %s %s %s %s %s %s\n' "$pid" "$user" "$cpu" "$mem" "$rss" "$start" "$name"; done; printf '@GPU\n'; if command -v nvidia-smi >/dev/null 2>&1; then nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader,nounits 2>/dev/null | head -16; fi; printf '@SOCKET\n'; cat /proc/net/sockstat; printf '@HANDLES\n'; cat /proc/sys/fs/file-nr; printf '@PORT\n'; ss -H -lntu 2>/dev/null | head -100; printf '@ADDR\n'; ip -o -4 addr show 2>/dev/null | head -128; printf '@TEMP\n'; for f in /sys/class/thermal/thermal_zone*/temp; do cat "$f" 2>/dev/null; done; true`

func parseMonitorDetails(raw, memory string) MonitorDetails {
	d := MonitorDetails{Sampled: time.Now(), Addresses: map[string]string{}}
	for _, line := range strings.Split(memory, "\n") {
		f := strings.Fields(line)
		if len(f) < 2 {
			continue
		}
		n := number(f[1]) * 1024
		switch f[0] {
		case "MemFree:":
			d.MemoryFree = n
		case "MemAvailable:":
			d.MemoryAvailable = n
		case "Cached:":
			d.Cached = n
		case "Buffers:":
			d.Buffers = n
		case "SwapTotal:":
			d.SwapTotal = n
		case "SwapFree:":
			d.SwapUsed = n
		}
	}
	if d.SwapUsed <= d.SwapTotal {
		d.SwapUsed = d.SwapTotal - d.SwapUsed
	} else {
		d.SwapUsed = 0
	}
	section := ""
	for _, line := range strings.Split(raw, "\n") {
		if strings.HasPrefix(line, "@") {
			section = strings.TrimSpace(line)
			continue
		}
		f := strings.Fields(line)
		switch section {
		case "@KERNEL":
			d.Kernel = strings.TrimSpace(line)
		case "@CPU":
			if len(f) < 9 || !strings.HasPrefix(f[0], "cpu") || len(d.CPUs) >= 257 {
				continue
			}
			c := CPUCounter{Name: f[0], User: number(f[1]) + number(f[2]), System: number(f[3]) + number(f[6]) + number(f[7]), Idle: number(f[4]), Wait: number(f[5])}
			for _, v := range f[1:9] {
				c.Total += number(v)
			}
			d.CPUs = append(d.CPUs, c)
		case "@NET":
			parts := strings.SplitN(line, ":", 2)
			if len(parts) != 2 || len(d.Networks) >= 128 {
				continue
			}
			v := strings.Fields(parts[1])
			if len(v) >= 16 {
				d.Networks = append(d.Networks, NetworkCounter{strings.TrimSpace(parts[0]), number(v[0]), number(v[8])})
			}
		case "@IO":
			if len(f) >= 14 && len(d.Disks) < 128 && !strings.HasPrefix(f[2], "loop") && !strings.HasPrefix(f[2], "ram") {
				d.Disks = append(d.Disks, DiskCounter{f[2], number(f[3]), number(f[7])})
			}
		case "@PART":
			if len(f) >= 6 && strings.HasPrefix(f[0], "/") && len(d.Partitions) < 128 {
				d.Partitions = append(d.Partitions, Partition{f[0], strings.Join(f[5:], " "), number(f[1]) * 1024, number(f[2]) * 1024, decimal(strings.TrimSuffix(f[4], "%"))})
			}
		case "@PROC":
			if len(f) >= 7 && len(d.Processes) < 40 {
				pid, _ := strconv.Atoi(f[0])
				if pid > 0 {
					d.Processes = append(d.Processes, ProcessMetric{pid, f[1], strings.Join(f[6:], " "), decimal(f[2]), decimal(f[3]), number(f[4]) * 1024, number(f[5])})
				}
			}
		case "@GPU":
			v := strings.Split(line, ",")
			if len(v) == 5 && len(d.GPUs) < 16 {
				d.GPUs = append(d.GPUs, GPU{strings.TrimSpace(v[0]), number(strings.TrimSpace(v[2])) << 20, number(strings.TrimSpace(v[3])) << 20, decimal(v[1]), decimal(v[4])})
			}
		case "@SOCKET":
			if len(f) > 2 {
				for i := 1; i+1 < len(f); i += 2 {
					switch f[0] + f[i] {
					case "sockets:used":
						d.Connections = number(f[i+1])
					case "TCP:inuse":
						d.TCPConnections = number(f[i+1])
					case "TCP:tw":
						d.TimeWait = number(f[i+1])
					case "UDP:inuse":
						d.UDPConnections = number(f[i+1])
					}
				}
			}
		case "@HANDLES":
			if len(f) == 3 {
				d.Handles = number(f[0])
				d.HandleLimit = number(f[2])
			}
		case "@PORT":
			if len(f) >= 5 && len(d.Listeners) < 100 {
				d.Listeners = append(d.Listeners, Listener{f[0], f[4]})
			}
		case "@ADDR":
			if len(f) >= 4 && len(d.Addresses) < 128 {
				d.Addresses[strings.TrimSuffix(f[1], ":")] = f[3]
			}
		case "@TEMP":
			n := decimal(line) / 1000
			if n > 0 && n < 150 && n > d.Temperature {
				d.Temperature = n
			}
		}
	}
	return d
}
func number(s string) uint64 { n, _ := strconv.ParseUint(s, 10, 64); return n }
func decimal(s string) float64 {
	n, _ := strconv.ParseFloat(strings.TrimSpace(s), 64)
	if n < 0 || n != n || n > 1e15 {
		return 0
	}
	return n
}
