package shell

import (
	"os/exec"
	"strings"
	"testing"
	"time"
)

const detailedFixture = `@KERNEL
Linux 6.8.0
@CPU
cpu 100 0 50 800 25 0 25 0 0 0
cpu0 50 0 25 400 12 0 13 0
@NET
 eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
@IO
 8 0 vda 100 0 0 0 200 0 0 0 0 0 0
@PART
/dev/vda 1000 250 750 25% /
@PROC
42 user 20.5 10 1024 999 worker name
@GPU
Example GPU, 30, 512, 1024, 45
@SOCKET
sockets: used 50
TCP: inuse 20 orphan 0 tw 5 alloc 25 mem 2
UDP: inuse 3 mem 2
@HANDLES
100 0 10000
@PORT
tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*
@ADDR
2: eth0 inet 192.0.2.1/24 brd 192.0.2.255
@TEMP
45000
`

func TestDetailedMetricsOptionalSections(t *testing.T) {
	m, err := parseMetrics(fixtureMetrics + "__DETAILS__\n" + detailedFixture)
	if err != nil {
		t.Fatal(err)
	}
	d := m.Details
	if len(d.CPUs) != 2 || d.CPUs[0].Total != 1000 || len(d.Networks) != 1 || len(d.GPUs) != 1 || d.GPUs[0].Used != 512<<20 || d.Processes[0].Start != 999 || d.Connections != 50 || d.TimeWait != 5 || len(d.Listeners) != 1 || d.Addresses["eth0"] != "192.0.2.1/24" || d.Temperature != 45 {
		t.Fatalf("incorrect details: %+v", d)
	}
	d = parseMonitorDetails("@GPU\nN/A\n@CPU\nmalformed\n", "SwapTotal: 100 kB\nSwapFree: 25 kB\nCached: 4 kB")
	if len(d.GPUs) != 0 || len(d.CPUs) != 0 || d.SwapUsed != 75*1024 {
		t.Fatal(d)
	}
	huge := strings.Repeat("cpu0 1 1 1 1 1 1 1 1\n", 1000)
	if len(parseMonitorDetails("@CPU\n"+huge, "").CPUs) != 257 {
		t.Fatal("core bound lost")
	}
}
func TestMonitorCounterRates(t *testing.T) {
	a := parseMonitorDetails(detailedFixture, "")
	b := parseMonitorDetails(detailedFixture, "")
	b.Sampled = a.Sampled.Add(2 * time.Second)
	b.CPUs[0].Total += 100
	b.CPUs[0].Idle += 60
	b.CPUs[0].User += 25
	b.CPUs[0].System += 10
	b.CPUs[0].Wait += 5
	b.Networks[0].Received += 2000
	b.Networks[0].Sent += 1000
	b.Disks[0].Reads += 10
	r := Rates(a, b)
	if !r.Ready || r.CPUs[0].Used != 40 || r.CPUs[0].User != 25 || r.Networks[0].Received != 1000 || r.Disks[0].Reads != 5 {
		t.Fatal(r)
	}
	b.Networks[0].Received = 1
	if Rates(a, b).Networks[0].Received != 0 {
		t.Fatal("counter reset overflow")
	}
	if Rates(MonitorDetails{}, b).Ready {
		t.Fatal("first sample has fabricated rate")
	}
}
func TestMonitorCommandsValidateAndQuote(t *testing.T) {
	for _, paths := range [][]string{nil, {"relative"}, {"/tmp\nattack"}} {
		if _, err := largeFilesCommand(paths, 100<<20, 20); err == nil {
			t.Fatal(paths)
		}
	}
	command, err := largeFilesCommand([]string{"/tmp/o'brien; $(echo no)"}, 100<<20, 20)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(command, "timeout 25s sh -c ") {
		t.Fatal("scan must have remote timeout")
	}
	for _, p := range []ProcessMetric{{PID: 1, Start: 4}, {PID: 4}} {
		if _, err := stopProcessCommand(p); err == nil {
			t.Fatal(p)
		}
	}
	stop, err := stopProcessCommand(ProcessMetric{PID: 42, Start: 999})
	if err != nil || !strings.Contains(stop, "kill -TERM -- 42") || !strings.Contains(stop, "= 999") {
		t.Fatal(stop, err)
	}
	if sh, err := exec.LookPath("sh"); err == nil {
		for _, c := range []string{command, stop, monitorCommand + detailedMonitorCommand} {
			if out, err := exec.Command(sh, "-n", "-c", c).CombinedOutput(); err != nil {
				t.Fatalf("invalid probe shell syntax: %v %s", err, out)
			}
		}
	}
}
