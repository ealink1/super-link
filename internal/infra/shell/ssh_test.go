package shell

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/pem"
	"errors"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/pkg/sftp"
	"golang.org/x/crypto/ssh"
)

type sshFixture struct {
	host        domain.ShellHost
	listener    net.Listener
	wg          sync.WaitGroup
	connections sync.Map
	resized     chan [2]uint32
	files       sftp.Handlers
	stall       atomic.Bool
	stalled     chan struct{}
	rejectPTY   atomic.Bool
	channels    atomic.Int64
}

func newSSHFixture(t *testing.T) *sshFixture {
	t.Helper()
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := ssh.NewSignerFromKey(key)
	if err != nil {
		t.Fatal(err)
	}
	config := &ssh.ServerConfig{PasswordCallback: func(_ ssh.ConnMetadata, password []byte) (*ssh.Permissions, error) {
		if string(password) != "fixture" {
			return nil, errors.New("authentication rejected")
		}
		return nil, nil
	}}
	config.PublicKeyCallback = func(_ ssh.ConnMetadata, key ssh.PublicKey) (*ssh.Permissions, error) { return nil, nil } // Isolated test server only.
	config.AddHostKey(signer)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	_, port, _ := net.SplitHostPort(listener.Addr().String())
	number, _ := strconv.Atoi(port)
	f := &sshFixture{host: domain.ShellHost{Name: "fixture", Host: "127.0.0.1", Port: number, User: "tester", Password: "fixture", Fingerprint: ssh.FingerprintSHA256(signer.PublicKey())}, listener: listener, resized: make(chan [2]uint32, 10), files: sftp.InMemHandler(), stalled: make(chan struct{})}
	f.wg.Add(1)
	go func() {
		defer f.wg.Done()
		for {
			connection, err := listener.Accept()
			if err != nil {
				return
			}
			f.connections.Store(connection, true)
			f.wg.Add(1)
			go func() {
				defer f.wg.Done()
				defer f.connections.Delete(connection)
				defer connection.Close()
				_, channels, requests, err := ssh.NewServerConn(connection, config)
				if err != nil {
					return
				}
				go ssh.DiscardRequests(requests)
				for next := range channels {
					f.channels.Add(1)
					channel, requests, err := next.Accept()
					if err != nil {
						continue
					}
					f.wg.Add(1)
					go func() { defer f.wg.Done(); f.serve(channel, requests) }()
				}
			}()
		}
	}()
	t.Cleanup(func() {
		listener.Close()
		f.connections.Range(func(key, value any) bool { key.(net.Conn).Close(); return true })
		f.wg.Wait()
	})
	return f
}

func TestSSHEncryptedPrivateKeyAuthentication(t *testing.T) {
	f := newSSHFixture(t)
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	block, err := ssh.MarshalPrivateKeyWithPassphrase(key, "fixture", []byte("fixture-passphrase"))
	if err != nil {
		t.Fatal(err)
	}
	filename := filepath.Join(t.TempDir(), "fixture-key")
	if err := os.WriteFile(filename, pem.EncodeToMemory(block), 0600); err != nil {
		t.Fatal(err)
	}
	h := f.host
	h.Password = ""
	h.KeyPath = filename
	h.Passphrase = "fixture-passphrase"
	r, err := OpenSSH(context.Background(), h)
	if err != nil {
		t.Fatal(err)
	}
	r.Close()
	h.Passphrase = "wrong"
	if _, err := OpenSSH(context.Background(), h); err == nil {
		t.Fatal("wrong key passphrase accepted")
	}
}

func TestProbeSSHVerifiesCredentialsWithoutASession(t *testing.T) {
	f := newSSHFixture(t)
	pinned := f.host.Fingerprint
	h := f.host
	h.Fingerprint = ""
	fingerprint, err := ProbeSSH(context.Background(), h)
	if err != nil {
		t.Fatal(err)
	}
	if fingerprint != pinned {
		t.Fatalf("probe reported %q, want %q", fingerprint, pinned)
	}
	if opened := f.channels.Load(); opened != 0 {
		t.Fatalf("probe opened %d session channels, want none", opened)
	}
	wrong := h
	wrong.Password = "not-the-fixture"
	if _, err = ProbeSSH(context.Background(), wrong); err == nil {
		t.Fatal("probe accepted the wrong password")
	}
	h.Fingerprint = "SHA256:changed-pin"
	var key *HostKeyError
	if _, err = ProbeSSH(context.Background(), h); !errors.As(err, &key) || !key.Changed {
		t.Fatalf("probe ignored a changed host key pin: %v", err)
	}
	remote, err := OpenSSH(context.Background(), f.host)
	if err != nil {
		t.Fatal(err)
	}
	if err = remote.Close(); err != nil {
		t.Fatal(err)
	}
	if f.channels.Load() == 0 {
		t.Fatal("the shared setup no longer opens a session")
	}
}
func (f *sshFixture) serve(channel ssh.Channel, requests <-chan *ssh.Request) {
	defer channel.Close()
	for request := range requests {
		switch request.Type {
		case "pty-req":
			request.Reply(!f.rejectPTY.Load(), nil)
		case "window-change":
			var size struct{ Columns, Rows, Width, Height uint32 }
			_ = ssh.Unmarshal(request.Payload, &size)
			f.resized <- [2]uint32{size.Columns, size.Rows}
		case "shell":
			request.Reply(true, nil)
			f.wg.Add(1)
			go func() { defer f.wg.Done(); _, _ = io.Copy(channel, channel) }()
		case "exec":
			request.Reply(true, nil)
			_, _ = io.WriteString(channel, fixtureMetrics)
			_, _ = channel.SendRequest("exit-status", false, ssh.Marshal(struct{ Status uint32 }{Status: 0}))
			return
		case "subsystem":
			request.Reply(true, nil)
			if f.stall.Load() {
				close(f.stalled)
				for range requests {
				}
				return
			}
			server := sftp.NewRequestServer(channel, f.files)
			_ = server.Serve()
			server.Close()
			return
		default:
			request.Reply(false, nil)
		}
	}
}

func TestAuxiliaryCancellationPreservesTerminal(t *testing.T) {
	f := newSSHFixture(t)
	r, err := OpenSSH(context.Background(), f.host)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	f.stall.Store(true)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() { _, err := r.Files(ctx, "/"); done <- err }()
	select {
	case <-f.stalled:
	case <-time.After(time.Second):
		t.Fatal("subsystem did not start")
	}
	cancel()
	select {
	case err := <-done:
		if err == nil {
			t.Fatal("cancel ignored")
		}
	case <-time.After(time.Second):
		t.Fatal("auxiliary read remained blocked")
	}
	if _, err := r.Write([]byte("alive")); err != nil {
		t.Fatal("cancel closed terminal", err)
	}
	output := make([]byte, 5)
	if _, err := io.ReadFull(r, output); err != nil || string(output) != "alive" {
		t.Fatal("terminal did not survive", err)
	}
}

func TestSSHHostIdentityAndInteractiveLifecycle(t *testing.T) {
	f := newSSHFixture(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	for _, fingerprint := range []string{"", "SHA256:changed"} {
		h := f.host
		h.Fingerprint = fingerprint
		_, err := OpenSSH(ctx, h)
		var key *HostKeyError
		if !errors.As(err, &key) || key.Changed != (fingerprint != "") {
			t.Fatal("identity error lost", err)
		}
	}
	r, err := OpenSSH(ctx, f.host)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	context.AfterFunc(ctx, func() { r.Close() })
	if err := r.Resize(91, 31); err != nil {
		t.Fatal(err)
	}
	select {
	case size := <-f.resized:
		if size != [2]uint32{91, 31} {
			t.Fatal(size)
		}
	case <-ctx.Done():
		t.Fatal("resize timeout")
	}
	if _, err := r.Write([]byte("你好\n")); err != nil {
		t.Fatal(err)
	}
	raw := make([]byte, len("你好\n"))
	if _, err := io.ReadFull(r, raw); err != nil || string(raw) != "你好\n" {
		t.Fatal(string(raw), err)
	}
	if m, err := r.Monitor(ctx); err != nil || m.MemoryUsed != 614400 {
		t.Fatal(m, err)
	}
	if err := r.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := r.Read(raw); err == nil {
		t.Fatal("closed SSH remained readable")
	}
}

func TestSFTPStreamsAndProtectsExistingFiles(t *testing.T) {
	f := newSSHFixture(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	r, err := OpenSSH(ctx, f.host)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	context.AfterFunc(ctx, func() { r.Close() })
	root := t.TempDir()
	source := filepath.Join(root, "source")
	payload := make([]byte, 150000)
	_, _ = rand.Read(payload)
	if err := os.WriteFile(source, payload, 0600); err != nil {
		t.Fatal(err)
	}
	if err := r.Upload(ctx, source, "/fixture", nil); err != nil {
		t.Fatal(err)
	}
	if err := r.Upload(ctx, source, "/fixture", nil); err == nil {
		t.Fatal("remote overwrite allowed")
	}
	files, err := r.Files(ctx, "/")
	if err != nil || len(files) != 1 || files[0].Size != int64(len(payload)) {
		t.Fatal(files, err)
	}
	destination := filepath.Join(root, "download")
	if err := r.Download(ctx, "/fixture", destination, nil); err != nil {
		t.Fatal(err)
	}
	loaded, _ := os.ReadFile(destination)
	if string(loaded) != string(payload) {
		t.Fatal("transfer corrupted")
	}
	if err := r.Download(ctx, "/fixture", destination, nil); err == nil {
		t.Fatal("local overwrite allowed")
	}
}

func TestSSHCancellationInterruptsHandshake(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	done := make(chan struct{})
	go func() {
		defer close(done)
		connection, err := listener.Accept()
		if err == nil {
			defer connection.Close()
			io.Copy(io.Discard, connection)
		}
	}()
	_, port, _ := net.SplitHostPort(listener.Addr().String())
	number, _ := strconv.Atoi(port)
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	_, err = OpenSSH(ctx, domain.ShellHost{Name: "stalled", Host: "127.0.0.1", Port: number, User: "tester"})
	if err == nil {
		t.Fatal("handshake ignored cancellation")
	}
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("handshake leaked socket")
	}
}
