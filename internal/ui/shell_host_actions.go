package ui

import (
	"context"
	"errors"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/secrets"
)

// The card menu floats beside its trigger so the host stays visible while an
// action is chosen; it opens below the button and flips above when clipped.
func (s *shellWorkspace) hostMenu(h domain.ShellHost, anchor fyne.CanvasObject) {
	var popup *widget.PopUp
	choose := func(run func()) func() { return func() { popup.Hide(); run() } }
	panel := container.NewThemeOverride(shellPanel(shellVBox(
		shellMenuRow("编辑", "pencil", false, true, choose(func() { s.loadHostEditor(h) })),
		shellMenuRow("复制密码", "copy", false, true, choose(func() { s.copyHostPassword(h) })),
		shellMenuRow("删除", "trash-2", true, true, choose(func() { s.deleteHost(h) })),
	), monitorSurfaceColor, 9, 4), newShellTheme())
	popup = widget.NewPopUp(panel, s.owner.Window.Canvas())
	canvas := s.owner.Window.Canvas()
	origin := fyne.CurrentApp().Driver().AbsolutePositionForObject(anchor)
	size := popup.MinSize()
	x := max(8, origin.X+anchor.Size().Width-size.Width)
	y := origin.Y + anchor.Size().Height + 6
	if y+size.Height > canvas.Size().Height-8 {
		y = max(8, origin.Y-size.Height-6)
	}
	popup.ShowAtPosition(fyne.NewPos(x, y))
}

func (s *shellWorkspace) copyHostPassword(h domain.ShellHost) {
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.service.Get(ctx, h.ID) }, func(value any, err error) {
		if err == nil {
			password := value.(domain.ShellHost).Password
			if password != "" {
				fyne.CurrentApp().Clipboard().SetContent(password)
				return
			}
			err = secrets.ErrMissing
		}
		if errors.Is(err, secrets.ErrMissing) || errors.Is(err, secrets.ErrLegacy) {
			showInformationDialog("复制密码", err.Error(), s.owner.Window)
			return
		}
		s.owner.showError(err)
	})
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
