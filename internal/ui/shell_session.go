package ui

import (
	"context"
	"errors"
	"io"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellPane struct {
	workspace     *shellWorkspace
	item          *container.TabItem
	terminal      *terminalSurface
	status        *widget.Label
	ctx           context.Context
	cancel        context.CancelFunc
	input         chan []byte
	sizes         chan [2]int
	session       transport.Session
	closed, ended bool
	aux           *fyne.Container
	filePane      *shellFiles
	monitorPane   *shellMonitor
	remoteActions [2]*shellAlignedButton
	executable    string
	host          *domain.ShellHost
	connection    *shellConnectDialog
	indicator     *connectionDot
	auxKind       string
}

func (s *shellWorkspace) newPane(name string) *shellPane {
	ctx, cancel := context.WithCancel(s.owner.jobs.ctx)
	p := &shellPane{workspace: s, ctx: ctx, cancel: cancel, input: make(chan []byte, 128), sizes: make(chan [2]int, 1), status: widget.NewLabel("正在连接…"), aux: container.NewStack()}
	p.status.Truncation = fyne.TextTruncateEllipsis
	p.terminal = newTerminalSurface(func(cols, rows int) {
		select {
		case p.sizes <- [2]int{cols, rows}:
		default:
			select {
			case <-p.sizes:
			default:
			}
			select {
			case p.sizes <- [2]int{cols, rows}:
			default:
			}
		}
	})
	p.terminal.rejectPaste = func() { s.owner.showError(errors.New("单次粘贴上限 64 KiB，请分段粘贴")) }
	p.terminal.textSize = float32(s.settings.FontSize)
	content := p.buildSessionContent()
	p.item = container.NewTabItem(name, content)
	s.panes[p.item] = p
	s.tabs.Append(p.item)
	s.tabs.Select(p.item)
	s.showTerminals()
	// Encoding may write terminal replies from the UI; this consumer always
	// drains the pipe, independently of slow or disconnected network writes.
	s.owner.jobs.run(func(context.Context) (any, error) {
		buffer := make([]byte, 4096)
		for {
			n, err := p.terminal.emulator.Read(buffer)
			if n > 0 {
				copy := append([]byte(nil), buffer[:n]...)
				select {
				case p.input <- copy:
				default:
					p.cancel()
				}
			}
			if err != nil {
				return nil, err
			}
		}
	}, func(_ any, _ error) {})
	context.AfterFunc(ctx, func() { _ = p.terminalPipeClose() })
	return p
}

func (p *shellPane) terminalPipeClose() error {
	if closer, ok := p.terminal.emulator.InputPipe().(io.Closer); ok {
		return closer.Close()
	}
	return nil
}

func (s *shellWorkspace) openLocal(executable string) {
	if executable == "" {
		executable = s.settings.DefaultShell
	}
	name := "本地终端"
	if executable != "" {
		name = executable
	}
	p := s.newPane(name)
	p.executable = executable
	s.owner.jobs.run(func(context.Context) (any, error) {
		session, err := transport.OpenLocal(p.ctx, executable)
		if err == nil {
			p.ownSession(session)
		}
		return session, err
	}, p.connected)
}

// Register ownership while the opening worker is still counted. Shutdown must
// join the closer even when cancellation beats the UI's connection callback.
func (p *shellPane) ownSession(session transport.Session) {
	jobs := p.workspace.owner.jobs
	jobs.wg.Add(1)
	jobs.workers.Add(1)
	go func() { defer jobs.wg.Done(); defer jobs.workers.Add(-1); <-p.ctx.Done(); session.Close() }()
}
func (p *shellPane) connected(value any, err error) {
	if p.connection != nil {
		p.connection.finish(err)
	}
	if p.closed {
		return
	}
	if err != nil {
		p.finish(err)
		return
	}
	p.session = value.(transport.Session)
	if _, ok := p.session.(*transport.Remote); ok {
		for _, button := range p.remoteActions {
			button.Enable()
		}
	}
	session := p.session
	p.setSessionState("已连接", false)
	if p.workspace.owner.switcher.mode == 1 && p.workspace.page == 1 && p.workspace.tabs.Selected() == p.item {
		p.workspace.owner.Window.Canvas().Focus(p.terminal)
	}
	select {
	case <-p.sizes:
	default:
	}
	p.sizes <- [2]int{p.terminal.emulator.Width(), p.terminal.emulator.Height()}
	p.workspace.owner.jobs.run(func(context.Context) (any, error) { return nil, p.writeSession(session) }, func(_ any, err error) {
		if err != nil && !p.closed {
			p.cancel()
			p.finish(err)
		}
	})
	p.workspace.owner.jobs.run(func(context.Context) (any, error) { defer session.Close(); return nil, p.readSession(session) }, func(_ any, err error) {
		if !p.closed {
			p.finish(err)
		}
	})
}
func (p *shellPane) writeSession(session transport.Session) error {
	for {
		select {
		case <-p.ctx.Done():
			return p.ctx.Err()
		case size := <-p.sizes:
			if err := session.Resize(size[0], size[1]); err != nil {
				return err
			}
		case input := <-p.input:
			n, err := session.Write(input)
			if err != nil {
				return err
			}
			if n != len(input) {
				return io.ErrShortWrite
			}
		}
	}
}
func (p *shellPane) readSession(session transport.Session) error {
	buffer := make([]byte, 8192)
	for {
		n, err := session.Read(buffer)
		if n > 0 {
			output := append([]byte(nil), buffer[:n]...)
			ack := make(chan struct{})
			p.workspace.owner.jobs.dispatch(func() {
				defer close(ack)
				if !p.closed {
					p.terminal.feed(output)
				}
			})
			select {
			case <-ack:
			case <-p.ctx.Done():
				return p.ctx.Err()
			}
		}
		if err != nil {
			return err
		}
	}
}
func (p *shellPane) finish(err error) {
	if p.closed {
		return
	}
	p.ended = true
	for _, button := range p.remoteActions {
		button.Disable()
	}
	p.cancel()
	p.terminal.dispose()
	if p.connection != nil {
		p.connection.finish(err)
	}
	if err == nil || errors.Is(err, io.EOF) {
		p.setSessionState("会话已结束", true)
	} else {
		p.setSessionState("会话已结束："+err.Error(), true)
	}
}
func (p *shellPane) stop() {
	if p.closed {
		return
	}
	p.closed = true
	if p.connection != nil {
		p.connection.close()
	}
	p.cancel()
	p.terminal.dispose()
}
