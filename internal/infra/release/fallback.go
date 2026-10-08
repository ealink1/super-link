package release

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"golang.org/x/mod/semver"
)

const releaseRepository = "https://github.com/ealink1/super-link/releases"

// HTTPError deliberately excludes response bodies and URLs, which can contain
// credentials from proxies or signed asset redirects.
type HTTPError struct {
	StatusCode  int
	RateLimited bool
}

func (e *HTTPError) Error() string {
	if e.RateLimited {
		return fmt.Sprintf("GitHub release HTTP %d (rate limit exceeded)", e.StatusCode)
	}
	return fmt.Sprintf("GitHub release HTTP %d", e.StatusCode)
}

func responseError(response *http.Response) *HTTPError {
	return &HTTPError{StatusCode: response.StatusCode, RateLimited: response.StatusCode == http.StatusTooManyRequests || response.Header.Get("X-RateLimit-Remaining") == "0"}
}

// Resolve latest once, then download both files from that exact tag. This avoids
// API rate limits and mixed versions if a release is published between requests.
func (c *Client) checkStableDownload(ctx context.Context) (Manifest, error) {
	tag, err := c.latestStableTag(ctx)
	if err != nil {
		return Manifest{}, err
	}
	base := releaseRepository + "/download/" + url.PathEscape(tag) + "/"
	return c.checkManifest(ctx, tag, base+"manifest.json", base+"manifest.json.sig")
}

func (c *Client) latestStableTag(ctx context.Context) (string, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, releaseRepository+"/latest", nil)
	if err != nil {
		return "", err
	}
	request.Header.Set("User-Agent", "SuperLink")
	request.Header.Set("Accept", "text/html")
	// Copy the client so concurrent checks/downloads keep their redirect policy.
	client := *c.HTTP
	client.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	response, err := client.Do(request)
	if err != nil {
		return "", err
	}
	defer response.Body.Close()
	if response.StatusCode == http.StatusNotFound {
		return "", ErrNoRelease
	}
	if response.StatusCode != http.StatusFound && response.StatusCode != http.StatusMovedPermanently && response.StatusCode != http.StatusSeeOther && response.StatusCode != http.StatusTemporaryRedirect && response.StatusCode != http.StatusPermanentRedirect {
		return "", responseError(response)
	}
	location, err := response.Location()
	if err != nil {
		return "", errors.New("latest release has no valid redirect")
	}
	if location.Scheme != "https" || location.Host != "github.com" || location.User != nil || location.RawQuery != "" || location.Fragment != "" {
		return "", errors.New("untrusted latest release redirect")
	}
	if location.Path == "/ealink1/super-link/releases" || location.Path == "/ealink1/super-link/releases/" {
		return "", ErrNoRelease
	}
	const prefix = "/ealink1/super-link/releases/tag/"
	if !strings.HasPrefix(location.Path, prefix) {
		return "", errors.New("latest release redirect is outside the configured repository")
	}
	tag := strings.TrimPrefix(location.Path, prefix)
	if !semver.IsValid("v"+strings.TrimPrefix(tag, "v")) || semver.Prerelease("v"+strings.TrimPrefix(tag, "v")) != "" {
		return "", errors.New("latest release redirect has no stable version tag")
	}
	return tag, nil
}
