package ui

import (
	"context"
	"sync/atomic"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/infra/chat"
)

type aiProgress struct {
	generation uint64
	text       string
}

// All panel state lives on the Fyne goroutine, except the immutable progress
// mailbox. The shared task supervisor cancels and joins HTTP work on exit.
type aiPanel struct {
	owner                                 *Window
	service                               *application.AISettings
	client                                *chat.Client
	config                                chat.Config
	content                               *container.ThemeOverride
	messages                              *fyne.Container
	scroll                                *container.Scroll
	input                                 *aiInput
	status, model                         *widget.Label
	sendButton, settingsButton, newButton *shellAlignedButton
	answer                                *widget.RichText
	history                               []chat.Message
	visibleText                           []string
	loading, saving, busy                 bool
	settingsOpen                          bool
	settingsDialog                        *dialog.CustomDialog
	generation                            uint64
	cancel                                context.CancelFunc
	progress                              atomic.Pointer[aiProgress]
}

func (w *Window) aiEntry() {
	if w.shuttingDown {
		return
	}
	s := w.switcher
	w.docTooltip.hide()
	if s.aiHost.Visible() {
		s.aiHost.Hide()
		w.Window.Canvas().Unfocus()
	} else {
		if s.ai == nil {
			s.ai = newAIPanel(w)
			s.aiHost.Objects = []fyne.CanvasObject{s.ai.content}
		}
		s.aiHost.Show()
		w.Window.Canvas().Focus(s.ai.input)
	}
	s.workspaceHost.Refresh()
}

func newAIPanel(w *Window) *aiPanel {
	p := &aiPanel{owner: w, service: &application.AISettings{Store: w.Store, Vault: w.Profiles.Vault}, client: chat.New(), loading: true}
	p.build()
	p.introduction()
	p.loadConfiguration()
	return p
}

func (p *aiPanel) loadConfiguration() {
	p.owner.jobs.run(func(ctx context.Context) (any, error) { return p.service.Load(ctx) }, func(value any, err error) {
		p.loading = false
		if err != nil {
			p.status.SetText(err.Error())
		} else {
			p.config = value.(chat.Config)
			p.showConfigurationStatus()
		}
		p.refreshControls()
	})
}

// A failed draft/note flush can keep the application open after workers were
// cancelled. Reconcile a possibly committed settings save and enable chat again.
func (w *Window) restartAI() {
	if p := w.switcher.ai; p != nil {
		if p.settingsDialog != nil {
			p.settingsDialog.Hide()
		}
		p.loading, p.saving = true, false
		p.refreshControls()
		p.loadConfiguration()
	}
}

func (p *aiPanel) showConfigurationStatus() {
	if p.config.Model == "" {
		p.model.SetText("普通问答 · 尚未配置")
		p.status.SetText("先打开 AI 设置，填写地址和模型")
		return
	}
	p.model.SetText(p.config.Model)
	p.status.SetText("已配置 · 发送后连接服务")
}

func (p *aiPanel) refreshControls() {
	p.settingsButton.Enable()
	p.newButton.Enable()
	if p.loading || p.saving || p.owner.shuttingDown {
		p.settingsButton.Disable()
		p.newButton.Disable()
		p.sendButton.Disable()
		return
	}
	if p.busy {
		p.settingsButton.Disable()
		p.sendButton.SetText("停止回复")
	} else {
		p.sendButton.SetText("发送")
	}
	p.sendButton.Enable()
}

func (p *aiPanel) newConversation() {
	if p.owner.shuttingDown || p.loading || p.saving {
		return
	}
	p.stop()
	p.history = nil
	p.answer = nil
	p.input.SetText("")
	p.introduction()
	p.showConfigurationStatus()
	p.refreshControls()
}

func (p *aiPanel) stop() {
	p.generation++
	if p.cancel != nil {
		p.cancel()
		p.cancel = nil
	}
	p.busy = false
	p.client.Close()
}
