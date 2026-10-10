package ui

import (
	"context"
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/layout"
	"github.com/ealink1/super-link/internal/domain"
)

func (s *shellWorkspace) hostActions(h domain.ShellHost) {
	var modal *shellModal
	close := func(run func()) { modal.hide(); run() }
	buttons := shellVBox(shellButton("连接终端", "terminal", true, func() { close(func() { s.connectHost(h) }) }), shellFixed(layout.NewSpacer(), 0, 12), shellOutlined(shellButton("编辑主机", "square-pen", false, func() { close(func() { s.loadHostEditor(h) }) })), shellFixed(layout.NewSpacer(), 0, 12), shellOutlined(shellButton("删除主机", "x", false, func() { close(func() { s.deleteHost(h) }) })))
	address := fmt.Sprintf("%s@%s:%d", h.User, h.Host, h.Port)
	if s.privacy {
		address = "••••@••••••"
	}
	body := shellVBox(shellText(address, 13, false, shellMutedColor), shellFixed(layout.NewSpacer(), 0, 24), buttons)
	modal = s.newModal(h.Name, body, shellButton("关闭", "", false, func() { modal.hide() }), nil)
	modal.popup.Resize(fyne.NewSize(420, 360))
	modal.popup.Show()
}

func (s *shellWorkspace) deleteHost(h domain.ShellHost) {
	showConfirmDialog("删除主机", "删除此主机及保存的凭据？", func(ok bool) {
		if !ok {
			return
		}
		s.owner.jobs.run(func(ctx context.Context) (any, error) { return nil, s.service.Delete(ctx, h.ID, h.Revision) }, func(_ any, err error) {
			if err != nil {
				s.owner.showError(err)
			}
			s.reloadHosts()
		})
	}, s.owner.Window)
}
