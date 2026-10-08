package ui

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"testing"

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
