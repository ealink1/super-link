package shell

type CPURate struct {
	Name                     string
	Used, User, System, Wait float64
}
type NetworkRate struct {
	Name           string
	Received, Sent float64
}
type DiskRate struct {
	Name          string
	Reads, Writes float64
}
type MonitorRates struct {
	CPUs     []CPURate
	Networks []NetworkRate
	Disks    []DiskRate
	Ready    bool
}

// Counter deltas avoid mistaking CPU load averages for utilization. Counter
// resets and disappearing devices do not produce negative or huge rates.
func Rates(previous, current MonitorDetails) MonitorRates {
	r := MonitorRates{}
	seconds := current.Sampled.Sub(previous.Sampled).Seconds()
	if previous.Sampled.IsZero() || seconds <= 0 {
		return r
	}
	r.Ready = true
	cpus := map[string]CPUCounter{}
	for _, c := range previous.CPUs {
		cpus[c.Name] = c
	}
	for _, c := range current.CPUs {
		p, ok := cpus[c.Name]
		if !ok || c.Total <= p.Total || c.Idle < p.Idle {
			continue
		}
		total := float64(c.Total - p.Total)
		r.CPUs = append(r.CPUs, CPURate{c.Name, min(100, 100*(1-float64(c.Idle-p.Idle)/total)), percentDelta(p.User, c.User, total), percentDelta(p.System, c.System, total), percentDelta(p.Wait, c.Wait, total)})
	}
	nets := map[string]NetworkCounter{}
	for _, n := range previous.Networks {
		nets[n.Name] = n
	}
	for _, n := range current.Networks {
		p, ok := nets[n.Name]
		if ok {
			r.Networks = append(r.Networks, NetworkRate{n.Name, rateDelta(p.Received, n.Received, seconds), rateDelta(p.Sent, n.Sent, seconds)})
		}
	}
	disks := map[string]DiskCounter{}
	for _, n := range previous.Disks {
		disks[n.Name] = n
	}
	for _, n := range current.Disks {
		p, ok := disks[n.Name]
		if ok {
			r.Disks = append(r.Disks, DiskRate{n.Name, rateDelta(p.Reads, n.Reads, seconds), rateDelta(p.Writes, n.Writes, seconds)})
		}
	}
	return r
}
func rateDelta(a, b uint64, seconds float64) float64 {
	if b < a {
		return 0
	}
	return float64(b-a) / seconds
}
func percentDelta(a, b uint64, total float64) float64 {
	if b < a {
		return 0
	}
	return min(100, 100*float64(b-a)/total)
}
