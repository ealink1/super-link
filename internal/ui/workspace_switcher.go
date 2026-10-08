package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

type workspaceSwitcher struct {
	owner         *Window
	sql           fyne.CanvasObject
	shell         *shellWorkspace
	note          *noteWorkspace
	body, bar     *fyne.Container
	buttons       [3]*workspaceModeButton
	themeButton   *widget.Button
	ai            *aiPanel
	aiHost        *fyne.Container
	workspaceHost *fyne.Container
	mode          int
	nativeSelect  func(int, bool)
	nativeClose   func()
}

func (w *Window) buildShell() {
	s := &workspaceSwitcher{owner: w, sql: w.buildSQLWorkspace()}
	w.switcher = s
	s.body = container.NewStack(s.sql)
	s.buttons[0] = newWorkspaceModeButton("SQL", func() { s.selectMode(0) })
	s.buttons[1] = newWorkspaceModeButton("Shell", func() { s.selectMode(1) })
	s.buttons[2] = newWorkspaceModeButton("Note", func() { s.selectMode(2) })
	s.buttons[0].selected = true
	menu := container.NewHBox(container.NewGridWrap(fyne.NewSize(56, 36), s.buttons[0]), container.NewGridWrap(fyne.NewSize(56, 36), s.buttons[1]), container.NewGridWrap(fyne.NewSize(56, 36), s.buttons[2]))
	s.bar = container.NewBorder(nil, nil, menu, s.utilityBar(), layout.NewSpacer())
	s.aiHost = container.NewStack()
	s.aiHost.Hide()
	s.workspaceHost = container.New(&aiWorkspaceLayout{}, s.body, s.aiHost)
	w.Window.SetContent(container.NewBorder(s.bar, nil, nil, nil, s.workspaceHost))
}

func (s *workspaceSwitcher) selectMode(mode int) {
	if s.owner.shuttingDown || mode < 0 || mode > 2 || s.mode == mode {
		return
	}
	s.owner.docTooltip.hide()
	s.owner.Window.Canvas().Unfocus()
	s.mode = mode
	s.owner.Window.SetPadded(mode == 0)
	if mode == 0 {
		s.body.Objects = []fyne.CanvasObject{s.sql}
	} else if mode == 1 {
		if s.shell == nil {
			s.shell = newShellWorkspace(s.owner)
		}
		s.body.Objects = []fyne.CanvasObject{s.shell.content}
	} else {
		if s.note == nil {
			s.note = newNoteWorkspace(s.owner)
		}
		s.body.Objects = []fyne.CanvasObject{s.note.content}
	}
	for i, button := range s.buttons {
		button.selected = false
		if i == mode {
			button.selected = true
		}
		button.Refresh()
	}
	s.body.Refresh()
	if s.nativeSelect != nil {
		s.nativeSelect(mode, s.owner.dark)
	}
}

func (s *workspaceSwitcher) installNative() {
	if s.nativeClose != nil {
		return
	}
	selectMode, closeTitlebar, ok := installWorkspaceTitlebar(s.owner.Window, s.selectMode, s.utilityAction)
	if ok {
		s.nativeSelect, s.nativeClose = selectMode, closeTitlebar
		s.nativeSelect(s.mode, s.owner.dark)
		s.bar.Hide()
	}
}

func (s *workspaceSwitcher) stop() {
	if s.ai != nil {
		s.ai.stop()
	}
	if s.shell != nil {
		s.shell.stop()
	}
}

func (w *Window) newWorkspaceConnection() {
	if w.switcher.mode == 2 {
		w.switcher.note.newNote()
		return
	}
	if w.switcher.mode == 1 {
		w.switcher.shell.editHost(domain.ShellHost{Port: 22, User: "root", Remember: true})
		return
	}
	w.editProfile(domain.Profile{})
}
