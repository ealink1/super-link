package ui

import (
	"context"
	"errors"

	"github.com/ealink1/super-link/internal/domain"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

type shellConnection struct {
	host    domain.ShellHost
	session *transport.Remote
}

func (s *shellWorkspace) connectHost(h domain.ShellHost) {
	s.connectVerifiedHost(h, nil)
}

// A one-time approval is bound to the exact host revision shown in the dialog.
// Reconnect reads the stored host again and requires another approval.
func (s *shellWorkspace) connectVerifiedHost(h domain.ShellHost, approval *shellHostApproval) {
	p := s.newPane(h.Name)
	p.host = &h
	p.connection = newShellConnectDialog(p)
	s.owner.jobs.run(func(context.Context) (any, error) {
		current, err := s.service.Get(p.ctx, h.ID)
		if err == nil {
			current, err = applyShellHostApproval(current, approval)
		}
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
	d := newSSHIdentityDialog(s.owner.Window, h, key.Fingerprint, func(ok, remember bool) {
		p.identity = nil
		if !ok || p.closed {
			return
		}
		s.owner.jobs.run(func(ctx context.Context) (any, error) {
			if !remember {
				return nil, nil
			}
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
			if remember {
				s.connectHost(h)
			} else {
				s.connectVerifiedHost(h, &shellHostApproval{host: h, fingerprint: key.Fingerprint})
			}
		})
	})
	p.identity = d
	d.show()
}

type shellHostApproval struct {
	host        domain.ShellHost
	fingerprint string
}

func applyShellHostApproval(current domain.ShellHost, approval *shellHostApproval) (domain.ShellHost, error) {
	if approval == nil {
		return current, nil
	}
	expected := approval.host
	if current.ID != expected.ID || current.Revision != expected.Revision || current.Host != expected.Host || current.Port != expected.Port || current.User != expected.User || current.Fingerprint != "" {
		return current, domain.ErrConflict
	}
	current.Fingerprint = approval.fingerprint
	return current, nil
}
