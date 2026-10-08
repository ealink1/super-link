package ui

import (
	"strings"

	transport "github.com/ealink1/super-link/internal/infra/shell"
)

// Keep host NICs (including virtual-machine NICs) in the overview, while leaving
// loopback and container-only plumbing available on the full NET page.
func monitorOverviewInterface(name string) bool {
	if name == "lo" || name == "lo0" || name == "" {
		return false
	}
	for _, prefix := range []string{"docker", "br-", "veth", "virbr", "vnet", "cni", "flannel", "cali", "kube-ipvs", "lxc", "lxdbr"} {
		if strings.HasPrefix(name, prefix) {
			return false
		}
	}
	return true
}
func monitorHostNetworkTotals(rates []transport.NetworkRate) (down, up float64) {
	for _, rate := range rates {
		if monitorOverviewInterface(rate.Name) {
			down += rate.Received
			up += rate.Sent
		}
	}
	return
}
