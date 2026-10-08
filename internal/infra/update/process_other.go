//go:build !windows

package update

import "os/exec"

func configureUpdateProcess(*exec.Cmd) {}
