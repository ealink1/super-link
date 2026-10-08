package ui

import (
	"testing"

	"fyne.io/fyne/v2"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func TestMonitorOverviewHidesContainerInterfacesWithoutLosingNETDetails(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	p := w.switcher.shell.newPane("fixture")
	defer p.stop()
	m := newShellMonitorView(p)
	names := []string{"lo", "enp1s0", "br-382a92d4dc44", "docker0", "veth123", "virbr0", "cni0", "flannel.1", "cali123", "kube-ipvs0"}
	for _, name := range names {
		m.metrics.Details.Networks = append(m.metrics.Details.Networks, transport.NetworkCounter{Name: name})
		m.rates.Networks = append(m.rates.Networks, transport.NetworkRate{Name: name, Received: 1024, Sent: 2048})
	}
	overview := m.overviewNetworks().(*fyne.Container)
	if len(overview.Objects) != 2 || overview.Objects[1].(*monitorDetailRow).title != "enp1s0" {
		t.Fatal("overview retained container plumbing", overview.Objects)
	}
	details := m.networkInterfaces().(*fyne.Container)
	if len(details.Objects) != len(names)+1 {
		t.Fatal("NET page lost interfaces")
	}
	down, up := monitorHostNetworkTotals(m.rates.Networks)
	if down != 1024 || up != 2048 {
		t.Fatal("container traffic was counted again", down, up)
	}
	// Unknown/custom names and several host NICs must not be silently discarded.
	for _, name := range []string{"eth0", "ens3", "enp2s0", "bond0", "br0", "wg0", "custom-uplink"} {
		if !monitorOverviewInterface(name) {
			t.Fatal("host interface hidden", name)
		}
	}
	m.rates.Ready = true
	row := overview.Objects[1].(*monitorDetailRow)
	if row.left != "↓ 1KB/s" || row.right != "↑ 2KB/s" {
		t.Fatal(row.left, row.right)
	}
}
