package release

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
	"time"
)

type transportFunc func(*http.Request) (*http.Response, error)

func (f transportFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func signedFixture(t *testing.T) (Manifest, []byte, []byte, string, ed25519.PrivateKey) {
	t.Helper()
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256([]byte("package"))
	m := Manifest{Schema: 1, Version: "0.2.0", Channel: "stable", PublishedAt: time.Now().UTC(), Artifacts: []Artifact{{ID: "superlink", Kind: "app", OS: "darwin", Arch: "arm64", Filename: "app.zip", URL: "https://github.com/ealink1/super-link/releases/download/v0.2.0/app.zip", Size: 7, SHA256: hex.EncodeToString(sum[:])}}}
	raw, _ := json.Marshal(m)
	sig := []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw)))
	return m, raw, sig, base64.StdEncoding.EncodeToString(public), private
}
func TestManifestAuthenticityAndStrictDecoding(t *testing.T) {
	m, raw, sig, key, private := signedFixture(t)
	if _, err := Verify(raw, sig, key); err != nil {
		t.Fatal(err)
	}
	tampered := append([]byte(nil), raw...)
	tampered[0] = ' '
	if _, err := Verify(tampered, sig, key); err == nil {
		t.Fatal("accepted altered manifest")
	}
	if _, err := Verify(raw, sig, ""); err == nil {
		t.Fatal("accepted absent trust key")
	}
	trailing := append(append([]byte(nil), raw...), []byte(" {}")...)
	trailingSig := []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, trailing)))
	if _, err := Verify(trailing, trailingSig, key); err == nil {
		t.Fatal("accepted trailing JSON")
	}
	m.Artifacts = append(m.Artifacts, m.Artifacts[0])
	if err := m.Validate(); err == nil {
		t.Fatal("accepted duplicate platform asset")
	}
	for _, address := range []string{"http://github.com/ealink1/super-link/releases/download/v1/a", "https://evil.test/a", "https://user:password@github.com/ealink1/super-link/releases/download/v1/a", "https://github.com/other/repo/releases/download/v1/a", "https://github.com:444/ealink1/super-link/releases/download/v1/a"} {
		if _, err := TrustedURL(address); err == nil {
			t.Fatal("trusted unsafe URL", address)
		}
	}
}
func TestLatestReleaseBindsTagToSignedManifest(t *testing.T) {
	_, raw, sig, key, _ := signedFixture(t)
	client := New(key)
	tag := "v0.2.0"
	client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
		body := raw
		switch {
		case strings.HasSuffix(r.URL.Path, "/latest"):
			body, _ = json.Marshal(map[string]any{"tag_name": tag, "assets": []map[string]string{{"name": "manifest.json", "browser_download_url": "https://github.com/ealink1/super-link/releases/download/v0.2.0/manifest.json"}, {"name": "manifest.json.sig", "browser_download_url": "https://github.com/ealink1/super-link/releases/download/v0.2.0/manifest.json.sig"}}})
		case strings.HasSuffix(r.URL.Path, ".sig"):
			body = sig
		}
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(string(body)))}, nil
	})
	if _, err := client.Check(context.Background()); err != nil {
		t.Fatal(err)
	}
	tag = "v0.3.0"
	if _, err := client.Check(context.Background()); err == nil {
		t.Fatal("accepted a substituted release tag")
	}
}
func TestDownloadVerifiesExactBytesAndCleansPartialFiles(t *testing.T) {
	m, _, _, _, _ := signedFixture(t)
	artifact := m.Artifacts[0]
	for _, body := range []string{"package", "tamper!", "short", "package-too-long"} {
		t.Run(body, func(t *testing.T) {
			client := New("")
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				return &http.Response{StatusCode: 200, Header: make(http.Header), ContentLength: -1, Body: io.NopCloser(strings.NewReader(body))}, nil
			})
			root := t.TempDir()
			file, err := client.Download(context.Background(), artifact, root)
			if body == "package" {
				if err != nil {
					t.Fatal(err)
				}
				data, _ := os.ReadFile(file)
				if string(data) != body {
					t.Fatal("wrong downloaded bytes")
				}
				if _, err = client.Download(context.Background(), artifact, root); err != nil {
					t.Fatal("retry replacement failed", err)
				}
			} else {
				if err == nil {
					t.Fatal("accepted invalid package")
				}
				entries, _ := os.ReadDir(root)
				if len(entries) != 0 {
					t.Fatal("partial download left on disk")
				}
			}
		})
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	client := New("")
	if _, err := client.Download(ctx, artifact, t.TempDir()); err == nil {
		t.Fatal("ignored cancelled download")
	}
}

func TestSignedDriverDownloadFromSeparateRepository(t *testing.T) {
	m, _, _, key, private := signedFixture(t)
	m.Artifacts[0].Kind = "driver"
	m.Artifacts[0].ID = "sqlite"
	m.Artifacts[0].Revision = "reviewed"
	m.Artifacts[0].Protocol = "json-lines-v2"
	m.Artifacts[0].URL = "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/v0.2.0/app.zip"
	raw, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	sig := []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw)))
	verified, err := Verify(raw, sig, key)
	if err != nil {
		t.Fatal(err)
	}
	client := New(key)
	client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
		if r.URL.String() != m.Artifacts[0].URL {
			t.Fatal("wrong driver repository")
		}
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("package"))}, nil
	})
	if _, err := client.Download(context.Background(), verified.Artifacts[0], t.TempDir()); err != nil {
		t.Fatal(err)
	}
	for _, address := range []string{
		"https://github.com/ealink1/SuperLink-DriverAgents-evil/releases/download/v0.2.0/app.zip",
		"https://github.com/other/SuperLink-DriverAgents/releases/download/v0.2.0/app.zip",
	} {
		if _, err := TrustedURL(address); err == nil {
			t.Fatal("accepted untrusted driver repository", address)
		}
	}
}
