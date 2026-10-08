package update

import (
	"os/exec"
	"syscall"
)

func configureUpdateProcess(command *exec.Cmd) {
	command.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
}
