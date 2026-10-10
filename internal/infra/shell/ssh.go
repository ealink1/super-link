package shell

import (
	"context"
	"crypto/subtle"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"sync"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	"golang.org/x/crypto/ssh"
)

// HostKeyError prevents silent trust and distinguishes changed server identities.
type HostKeyError struct {
	Fingerprint string
	Changed     bool
}

func (e *HostKeyError) Error() string {
	if e.Changed {
		return "SSH 主机指纹发生变化，连接已阻止，请核实服务器身份"
	}
	return "首次连接需要核实 SSH 主机指纹：" + e.Fingerprint
}

// Remote is an SSH PTY, with auxiliary channels sharing its verified client.
type Remote struct {
	client  *ssh.Client
	session *ssh.Session
	input   io.WriteCloser
	output  *io.PipeReader
	writer  *io.PipeWriter
	done    chan struct{}
	once    sync.Once
}

// OpenSSH authenticates with a pinned host key and opens an interactive PTY.
func OpenSSH(ctx context.Context, h domain.ShellHost) (*Remote, error) {
	return OpenSSHWithProgress(ctx, h, nil)
}

func openSSH(ctx context.Context, h domain.ShellHost, report func(ConnectStage, ConnectState)) (*Remote, error) {
	remote, _, err := setupSSH(ctx, h, func(key ssh.PublicKey) error {
		fingerprint := ssh.FingerprintSHA256(key)
		if subtle.ConstantTimeCompare([]byte(h.Fingerprint), []byte(fingerprint)) != 1 {
			return &HostKeyError{Fingerprint: fingerprint, Changed: h.Fingerprint != ""}
		}
		return nil
	}, report, true)
	return remote, err
}

// ProbeSSH authenticates and reports the server fingerprint without opening a
// session, so checking credentials leaves no remote shell behind. A host that
// already carries a pinned fingerprint must still present it; an unpinned one is
// reported for trust-on-first-use and never stored here.
func ProbeSSH(ctx context.Context, h domain.ShellHost) (string, error) {
	_, fingerprint, err := setupSSH(ctx, h, func(key ssh.PublicKey) error {
		observed := ssh.FingerprintSHA256(key)
		if h.Fingerprint != "" && subtle.ConstantTimeCompare([]byte(h.Fingerprint), []byte(observed)) != 1 {
			return &HostKeyError{Fingerprint: observed, Changed: true}
		}
		return nil
	}, func(ConnectStage, ConnectState) {}, false)
	return fingerprint, err
}

// setupSSH performs the bounded TCP and SSH setup. trust decides whether the
// server identity is acceptable, and wantPTY=false stops after authentication,
// which is when a credential failure is already known.
func setupSSH(ctx context.Context, h domain.ShellHost, trust func(ssh.PublicKey) error, report func(ConnectStage, ConnectState), wantPTY bool) (*Remote, string, error) {
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	if err := h.Validate(); err != nil {
		return nil, "", err
	}
	auth, err := sshAuth(h)
	if err != nil {
		return nil, "", err
	}
	address := net.JoinHostPort(h.Host, strconv.Itoa(h.Port))
	connection, err := (&net.Dialer{Timeout: 15 * time.Second}).DialContext(ctx, "tcp", address)
	if err != nil {
		return nil, "", fmt.Errorf("连接 SSH：%w", err)
	}
	report(ConnectTCP, ConnectCompleted)
	report(ConnectAuth, ConnectStarted)
	stop := context.AfterFunc(ctx, func() { connection.Close() })
	defer stop()
	if err := connection.SetDeadline(time.Now().Add(15 * time.Second)); err != nil {
		connection.Close()
		return nil, "", err
	}
	fingerprint := ""
	config := &ssh.ClientConfig{User: h.User, Auth: auth, HostKeyCallback: func(_ string, _ net.Addr, key ssh.PublicKey) error {
		fingerprint = ssh.FingerprintSHA256(key)
		return trust(key)
	}}
	clientConn, channels, requests, err := ssh.NewClientConn(connection, address, config)
	if err != nil {
		connection.Close()
		return nil, fingerprint, fmt.Errorf("SSH 握手：%w", err)
	}
	client := ssh.NewClient(clientConn, channels, requests)
	report(ConnectAuth, ConnectCompleted)
	if err := connection.SetDeadline(time.Time{}); err != nil {
		client.Close()
		return nil, fingerprint, err
	}
	if !wantPTY {
		return nil, fingerprint, client.Close()
	}
	report(ConnectChannel, ConnectStarted)
	remote, err := openPTY(client)
	if err != nil {
		client.Close()
		return nil, fingerprint, err
	}
	report(ConnectChannel, ConnectCompleted)
	report(ConnectReady, ConnectStarted)
	if err := ctx.Err(); err != nil {
		return nil, fingerprint, errors.Join(err, remote.Close())
	}
	report(ConnectReady, ConnectCompleted)
	return remote, fingerprint, nil
}

func sshAuth(h domain.ShellHost) ([]ssh.AuthMethod, error) {
	if h.KeyPath == "" {
		return []ssh.AuthMethod{ssh.Password(h.Password)}, nil
	}
	file, err := os.Open(h.KeyPath)
	if err != nil {
		return nil, fmt.Errorf("读取私钥：%w", err)
	}
	defer file.Close()
	raw, err := io.ReadAll(io.LimitReader(file, (2<<20)+1))
	defer clear(raw)
	if err != nil || len(raw) > 2<<20 {
		return nil, errors.New("私钥无法读取或超过 2 MiB")
	}
	var signer ssh.Signer
	if h.Passphrase != "" {
		signer, err = ssh.ParsePrivateKeyWithPassphrase(raw, []byte(h.Passphrase))
	} else {
		signer, err = ssh.ParsePrivateKey(raw)
	}
	if err != nil {
		return nil, errors.New("无法解析私钥，请检查格式和私钥口令")
	}
	return []ssh.AuthMethod{ssh.PublicKeys(signer)}, nil
}

func openPTY(client *ssh.Client) (*Remote, error) {
	session, err := client.NewSession()
	if err != nil {
		return nil, err
	}
	input, err := session.StdinPipe()
	if err != nil {
		session.Close()
		return nil, err
	}
	reader, writer := io.Pipe()
	session.Stdout, session.Stderr = writer, writer
	if err := session.RequestPty("xterm-256color", 24, 80, ssh.TerminalModes{ssh.ECHO: 1}); err != nil {
		session.Close()
		reader.Close()
		writer.Close()
		return nil, err
	}
	if err := session.Shell(); err != nil {
		session.Close()
		reader.Close()
		writer.Close()
		return nil, err
	}
	r := &Remote{client: client, session: session, input: input, output: reader, writer: writer, done: make(chan struct{})}
	go func() {
		defer close(r.done)
		err := session.Wait()
		writer.CloseWithError(err)
	}()
	return r, nil
}

// Read receives terminal output from the SSH PTY.
func (r *Remote) Read(p []byte) (int, error) { return r.output.Read(p) }

// Write sends encoded terminal input to the SSH PTY.
func (r *Remote) Write(p []byte) (int, error) { return r.input.Write(p) }

// Resize synchronizes the remote PTY geometry.
func (r *Remote) Resize(cols, rows int) error { return r.session.WindowChange(rows, cols) }

// Close interrupts transport I/O and joins the PTY's waiter.
func (r *Remote) Close() error {
	var err error
	r.once.Do(func() {
		// Close the transport first so stalled channels and pipe copies unblock.
		err = r.client.Close()
		err = errors.Join(err, r.output.Close(), r.writer.Close())
		<-r.done
	})
	return err
}
