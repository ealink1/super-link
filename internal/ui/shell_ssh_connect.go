package ui

import (
	"context"
	"errors"
	"fmt"

	"github.com/ealink1/super-link/internal/domain"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellConnection struct {
	host    domain.ShellHost
	session *transport.Remote
}

func (s *shellWorkspace) connectHost(h domain.ShellHost) {
	p := s.newPane(h.Name)
	p.host = &h
	p.connection = newShellConnectDialog(p)
	s.owner.jobs.run(func(context.Context) (any, error) {
		current, err := s.service.Get(p.ctx, h.ID)
		if err != nil {
			p.connection.model.record(transport.ConnectEvent{Stage: transport.ConnectTCP, State: transport.ConnectFailed})
			return shellConnection{}, err
		}
		session, err := transport.OpenSSHWithProgress(p.ctx, current, p.connection.model.record)
		if err == nil {
			p.ownSession(session)
		}
		current.Password, current.Passphrase = "", ""
		return shellConnection{host: current, session: session}, err
	}, func(value any, err error) {
		if p.closed || p.ended {
			return
		}
		result := value.(shellConnection)
		var key *transport.HostKeyError
		if errors.As(err, &key) && !key.Changed {
			p.finish(err)
			p.connection.close()
			s.confirmHostIdentity(p, result.host, key)
			return
		}
		p.connected(result.session, err)
	})
}

func (s *shellWorkspace) confirmHostIdentity(p *shellPane, h domain.ShellHost, key *transport.HostKeyError) {
	showConfirmDialog("核实 SSH 主机指纹", fmt.Sprintf("%s@%s:%d\n%s\n\n请通过可信渠道核实后，确认并连接。", h.User, h.Host, h.Port, key.Fingerprint), func(ok bool) {
		if !ok || p.closed {
			return
		}
		s.owner.jobs.run(func(ctx context.Context) (any, error) {
			return nil, s.service.Trust(ctx, h.ID, h.Revision, key.Fingerprint)
		}, func(_ any, err error) {
			if p.closed {
				return
			}
			if err != nil {
				s.owner.showError(err)
				return
			}
			p.stop()
			s.tabs.Remove(p.item)
			delete(s.panes, p.item)
			s.reloadHosts()
			s.connectHost(h)
		})
	}, s.owner.Window)
}
