package ui

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"net"
	"strconv"
	"strings"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"golang.org/x/crypto/ssh"
)

func TestSSHConfirmationUsesConnectedEndpointInsteadOfStaleList(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := ssh.NewSignerFromKey(key)
	if err != nil {
		t.Fatal(err)
	}
	config := &ssh.ServerConfig{NoClientAuth: true}
	config.AddHostKey(signer)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	serverDone := make(chan struct{})
	go func() {
		defer close(serverDone)
		connection, err := listener.Accept()
		if err != nil {
			return
		}
		defer connection.Close()
		connection.SetDeadline(time.Now().Add(5 * time.Second))
		_, _, _, _ = ssh.NewServerConn(connection, config)
	}()
	_, port, _ := net.SplitHostPort(listener.Addr().String())
	number, _ := strconv.Atoi(port)
	h, err := s.service.Save(context.Background(), domain.ShellHost{Name: "fixture", Host: "127.0.0.1", Port: number, User: "old-user"})
	if err != nil {
		t.Fatal(err)
	}
	current := h
	current.Host = "localhost"
	current.User = "current-user"
	if _, err := s.service.Save(context.Background(), current); err != nil {
		t.Fatal(err)
	}
	s.connectHost(h)
	pane := s.panes[s.tabs.Selected()]
	pumpShell(t, w, func() bool { return pane.ended })
	text := ""
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		text += shellDialogText(overlay)
	}
	if !strings.Contains(text, "current-user@localhost:") || strings.Contains(text, "old-user@127.0.0.1:") || !strings.Contains(strings.ReplaceAll(text, " ", ""), ssh.FingerprintSHA256(signer.PublicKey())) {
		t.Fatal("confirmation does not describe negotiated endpoint", text)
	}
	select {
	case <-serverDone:
	case <-time.After(time.Second):
		t.Fatal("failed handshake leaked server connection")
	}
}

func shellDialogText(object fyne.CanvasObject) string {
	switch object := object.(type) {
	case *widget.Label:
		return object.Text
	case *widget.RichText:
		return object.String()
	case *fyne.Container:
		var text strings.Builder
		for _, child := range object.Objects {
			text.WriteString(shellDialogText(child))
		}
		return text.String()
	case *container.Scroll:
		return shellDialogText(object.Content)
	case fyne.Widget:
		var text strings.Builder
		for _, child := range object.CreateRenderer().Objects() {
			text.WriteString(shellDialogText(child))
		}
		return text.String()
	}
	return ""
}

func TestSSHIdentityTrustChoiceControlsRealConnectionPersistence(t *testing.T) {
	for _, remember := range []bool{false, true} {
		t.Run(map[bool]string{false: "this connection", true: "remember host"}[remember], func(t *testing.T) {
			w := shellTestWindow(t)
			w.switcher.selectMode(1)
			waitUI(t, w)
			s := w.switcher.shell
			_, private, err := ed25519.GenerateKey(rand.Reader)
			if err != nil {
				t.Fatal(err)
			}
			signer, err := ssh.NewSignerFromKey(private)
			if err != nil {
				t.Fatal(err)
			}
			config := &ssh.ServerConfig{NoClientAuth: true}
			config.AddHostKey(signer)
			listener, err := net.Listen("tcp", "127.0.0.1:0")
			if err != nil {
				t.Fatal(err)
			}
			serverDone := make(chan struct{})
			go func() {
				defer close(serverDone)
				for {
					connection, err := listener.Accept()
					if err != nil {
						return
					}
					connection.SetDeadline(time.Now().Add(3 * time.Second))
					client, channels, requests, err := ssh.NewServerConn(connection, config)
					if err == nil {
						go ssh.DiscardRequests(requests)
						for channel := range channels {
							channel.Reject(ssh.UnknownChannelType, "fixture has no shell")
							break
						}
						client.Close()
					}
					connection.Close()
				}
			}()
			defer func() {
				listener.Close()
				select {
				case <-serverDone:
				case <-time.After(4 * time.Second):
					t.Error("SSH fixture did not stop")
				}
			}()
			_, port, _ := net.SplitHostPort(listener.Addr().String())
			number, _ := strconv.Atoi(port)
			h, err := s.service.Save(context.Background(), domain.ShellHost{Name: "identity choice", Host: "127.0.0.1", Port: number, User: "fixture"})
			if err != nil {
				t.Fatal(err)
			}
			s.connectHost(h)
			pane := s.panes[s.tabs.Selected()]
			pumpShell(t, w, func() bool { return pane.identity != nil })
			d := pane.identity
			d.remember.SetChecked(remember)
			d.confirm.OnTapped()
			pumpShell(t, w, func() bool { return pane.closed })
			next := s.panes[s.tabs.Selected()]
			pumpShell(t, w, func() bool { return next.ended })
			if next.identity != nil {
				t.Fatal("approved retry prompted again instead of verifying the pin")
			}
			stored, err := s.service.Get(context.Background(), h.ID)
			if err != nil {
				t.Fatal(err)
			}
			expected := ""
			if remember {
				expected = ssh.FingerprintSHA256(signer.PublicKey())
			}
			if stored.Fingerprint != expected {
				t.Fatal("trust choice did not control persistence")
			}
			if !remember {
				s.connectHost(h)
				last := s.panes[s.tabs.Selected()]
				pumpShell(t, w, func() bool { return last.identity != nil })
				last.stop()
				if !last.closed || !last.terminal.closed || last.identity != nil || d.popup.Visible() {
					t.Fatal("closing a pane leaked its identity dialog")
				}
			}
		})
	}
}
