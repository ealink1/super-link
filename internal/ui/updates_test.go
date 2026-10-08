package ui

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"runtime"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ealink1/super-link/internal/infra/release"
)

func TestUpdateCheckErrorExplainsGitHubFailures(t *testing.T) {
	for _, test := range []struct {
		status  int
		limited bool
		want    string
	}{
		{http.StatusForbidden, false, "GitHub 拒绝了更新请求"},
		{http.StatusForbidden, true, "GitHub 更新请求已被限流"},
		{http.StatusTooManyRequests, true, "GitHub 更新请求已被限流"},
		{http.StatusNotFound, false, "签名清单"},
	} {
		cause := &release.HTTPError{StatusCode: test.status, RateLimited: test.limited}
		wrapped := fmt.Errorf("check failed: %w", cause)
		err := updateCheckError(wrapped)
		if !strings.Contains(err.Error(), test.want) || !errors.Is(err, cause) {
			t.Fatal("missing localized explanation or error identity", err)
		}
	}
	if updateCheckError(context.Canceled) != context.Canceled {
		t.Fatal("lost cancellation identity")
	}
	if err := updateCheckError(release.ErrNoRelease); !strings.Contains(err.Error(), "公开稳定版本") || !errors.Is(err, release.ErrNoRelease) {
		t.Fatal("no public release explanation missing", err)
	}
}

type updateTestTransport func(*http.Request) (*http.Response, error)

func (f updateTestTransport) RoundTrip(request *http.Request) (*http.Response, error) {
	return f(request)
}

func TestCheckUpdatesShowsNoReleaseAfterForbiddenAPI(t *testing.T) {
	w := shellTestWindow(t)
	w.Releases.HTTP.Transport = updateTestTransport(func(request *http.Request) (*http.Response, error) {
		status, header := http.StatusForbidden, make(http.Header)
		if request.URL.Host == "github.com" {
			status = http.StatusFound
			header.Set("Location", "https://github.com/ealink1/super-link/releases")
		}
		return &http.Response{StatusCode: status, Header: header, Body: io.NopCloser(strings.NewReader("")), Request: request}, nil
	})
	w.checkUpdates()
	waitUI(t, w)
	var text strings.Builder
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		text.WriteString(shellDialogText(overlay))
	}
	if !strings.Contains(w.status.Text, "尚未发布公开稳定版本") || !strings.Contains(text.String(), "暂无可用的公开稳定版本") || strings.Contains(text.String(), "403") {
		t.Fatal("update check did not complete with a no-release explanation", w.status.Text, text.String())
	}
}

func TestStartupUpdateCheckRunsOnceAndStaysQuietOnNoRelease(t *testing.T) {
	w := shellTestWindow(t)
	var calls atomic.Int32
	w.Releases.HTTP.Transport = updateTestTransport(func(request *http.Request) (*http.Response, error) {
		calls.Add(1)
		return &http.Response{StatusCode: http.StatusNotFound, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("")), Request: request}, nil
	})
	status := w.status.Text
	overlays := len(w.Window.Canvas().Overlays().List())
	w.CheckUpdatesOnStartup()
	w.CheckUpdatesOnStartup()
	waitUI(t, w)
	if calls.Load() != 1 || w.updateChecking {
		t.Fatal("startup check did not run exactly once", calls.Load())
	}
	if w.status.Text != status || len(w.Window.Canvas().Overlays().List()) != overlays {
		t.Fatal("quiet check interrupted startup")
	}
}

func TestStartupUpdateShowsSignedNewVersionConfirmation(t *testing.T) {
	w := shellTestWindow(t)
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	w.Version = "0.1.0"
	manifest := release.Manifest{Schema: 1, Version: "0.2.0", Channel: "stable", PublishedAt: time.Now().UTC(), Artifacts: []release.Artifact{{ID: "superlink", Kind: "app", OS: runtime.GOOS, Arch: runtime.GOARCH, Filename: "SuperLink.zip", URL: "https://github.com/ealink1/super-link/releases/download/v0.2.0/SuperLink.zip", Size: 7, SHA256: strings.Repeat("a", 64)}}}
	raw, err := json.Marshal(manifest)
	if err != nil {
		t.Fatal(err)
	}
	signature := base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw))
	w.Releases.PublicKey = base64.StdEncoding.EncodeToString(public)
	w.Releases.HTTP.Transport = updateTestTransport(func(request *http.Request) (*http.Response, error) {
		body := string(raw)
		if strings.HasSuffix(request.URL.Path, "/latest") {
			info := map[string]any{"tag_name": "v0.2.0", "assets": []map[string]string{{"name": "manifest.json", "browser_download_url": "https://github.com/ealink1/super-link/releases/download/v0.2.0/manifest.json"}, {"name": "manifest.json.sig", "browser_download_url": "https://github.com/ealink1/super-link/releases/download/v0.2.0/manifest.json.sig"}}}
			data, _ := json.Marshal(info)
			body = string(data)
		} else if strings.HasSuffix(request.URL.Path, ".sig") {
			body = signature
		}
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body)), Request: request}, nil
	})
	w.CheckUpdatesOnStartup()
	waitUI(t, w)
	var text strings.Builder
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		text.WriteString(shellDialogText(overlay))
	}
	if !strings.Contains(text.String(), "0.1.0 → 0.2.0") || !strings.Contains(text.String(), "签名清单验证通过") {
		t.Fatal("missing update confirmation", text.String())
	}
}
