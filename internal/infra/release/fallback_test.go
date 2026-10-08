package release

import (
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"testing"
)

type trackedBody struct {
	io.Reader
	closed bool
}

func (b *trackedBody) Close() error { b.closed = true; return nil }

func TestForbiddenAndRateLimitedChecksUsePinnedSignedRelease(t *testing.T) {
	for _, code := range []int{http.StatusForbidden, http.StatusTooManyRequests} {
		t.Run(http.StatusText(code), func(t *testing.T) {
			_, raw, signature, key, _ := signedFixture(t)
			client := New(key)
			var addresses []string
			var bodies []*trackedBody
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				addresses = append(addresses, r.URL.String())
				if r.Header.Get("User-Agent") != "SuperLink" {
					t.Fatal("missing product user agent")
				}
				status, header, body := 200, make(http.Header), raw
				switch r.URL.String() {
				case latestAPI:
					status, body = code, []byte("proxy-secret")
					header.Set("X-RateLimit-Remaining", "0")
				case releaseRepository + "/latest":
					status, body = 302, nil
					header.Set("Location", releaseRepository+"/tag/v0.2.0")
				case releaseRepository + "/download/v0.2.0/manifest.json":
				case releaseRepository + "/download/v0.2.0/manifest.json.sig":
					body = signature
				default:
					t.Fatalf("unexpected request %s", r.URL.Path)
				}
				reader := &trackedBody{Reader: strings.NewReader(string(body))}
				bodies = append(bodies, reader)
				return &http.Response{StatusCode: status, Header: header, Body: reader, Request: r}, nil
			})
			manifest, err := client.Check(context.Background())
			if err != nil || manifest.Version != "0.2.0" || len(addresses) != 4 {
				t.Fatal("fallback did not verify pinned release", manifest.Version, err, addresses)
			}
			for _, body := range bodies {
				if !body.closed {
					t.Fatal("release response was not closed")
				}
			}
		})
	}
}

func TestFallbackRejectsUnsafeOrUnstableRedirects(t *testing.T) {
	for _, location := range []string{
		"https://evil.test/ealink1/super-link/releases/tag/v0.2.0",
		"http://github.com/ealink1/super-link/releases/tag/v0.2.0",
		"https://user:secret@github.com/ealink1/super-link/releases/tag/v0.2.0",
		"https://github.com:444/ealink1/super-link/releases/tag/v0.2.0",
		"https://github.com/other/repo/releases/tag/v0.2.0",
		releaseRepository + "/tag/v0.2.0?token=secret",
		releaseRepository + "/tag/v0.2.0#fragment",
		releaseRepository + "/tag/v0.2.0-preview",
		releaseRepository + "/tag/v0.2.0/extra",
		releaseRepository + "/tag/invalid", "",
	} {
		t.Run(location, func(t *testing.T) {
			client := New("")
			requests := 0
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				requests++
				status, header := 403, make(http.Header)
				if r.URL.String() == releaseRepository+"/latest" {
					status = 302
					header.Set("Location", location)
				}
				return &http.Response{StatusCode: status, Header: header, Body: io.NopCloser(strings.NewReader("")), Request: r}, nil
			})
			_, err := client.Check(context.Background())
			if err == nil || requests != 2 || strings.Contains(err.Error(), "secret") {
				t.Fatal("unsafe redirect was followed or leaked", requests, err)
			}
		})
	}
}

func TestFallbackPreservesManifestValidation(t *testing.T) {
	for _, failure := range []string{"signature", "version", "preview", "oversize", "missing"} {
		t.Run(failure, func(t *testing.T) {
			manifest, raw, signature, key, private := signedFixture(t)
			switch failure {
			case "signature":
				signature = []byte("invalid")
			case "version":
				manifest.Version = "0.3.0"
			case "preview":
				manifest.Channel = "preview"
			}
			if failure == "version" || failure == "preview" {
				raw, _ = json.Marshal(manifest)
				signature = []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw)))
			}
			client := New(key)
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				status, header, body := 200, make(http.Header), raw
				switch {
				case r.URL.String() == latestAPI:
					status = 403
				case r.URL.String() == releaseRepository+"/latest":
					status = 302
					header.Set("Location", "/ealink1/super-link/releases/tag/v0.2.0")
				case strings.HasSuffix(r.URL.Path, ".sig"):
					body = signature
				default:
					if failure == "missing" {
						status = 404
					}
					if failure == "oversize" {
						body = []byte(strings.Repeat("x", (1<<20)+1))
					}
				}
				return &http.Response{StatusCode: status, Header: header, Body: io.NopCloser(strings.NewReader(string(body))), Request: r}, nil
			})
			if _, err := client.Check(context.Background()); err == nil || errors.Is(err, ErrNoRelease) {
				t.Fatal("invalid fallback manifest accepted or reported as no release", err)
			}
		})
	}
}

func TestFallbackNoPublicReleaseAndAccessErrors(t *testing.T) {
	for _, scenario := range []string{"no-release", "drafts-only", "forbidden", "rate-limit"} {
		t.Run(scenario, func(t *testing.T) {
			client := New("")
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				status, header := 403, make(http.Header)
				header.Set("X-RateLimit-Remaining", "0")
				if r.URL.String() == releaseRepository+"/latest" {
					switch scenario {
					case "no-release":
						status = 404
					case "drafts-only":
						status = 302
						header.Set("Location", releaseRepository)
					case "rate-limit":
						status = 429
					}
				}
				return &http.Response{StatusCode: status, Header: header, Body: io.NopCloser(strings.NewReader("proxy-secret")), Request: r}, nil
			})
			_, err := client.Check(context.Background())
			if scenario == "no-release" || scenario == "drafts-only" {
				if !errors.Is(err, ErrNoRelease) {
					t.Fatal("no public release not recognized", err)
				}
			} else {
				var status *HTTPError
				if !errors.As(err, &status) || !status.RateLimited || !strings.Contains(err.Error(), "fallback failed") || strings.Contains(err.Error(), "proxy-secret") {
					t.Fatal("missing safe actionable status", err)
				}
			}
		})
	}
}

func TestFallbackHonorsCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	client := New("")
	requests := 0
	client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
		requests++
		if err := r.Context().Err(); err != nil {
			return nil, err
		}
		cancel()
		return &http.Response{StatusCode: 403, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("")), Request: r}, nil
	})
	if _, err := client.Check(ctx); !errors.Is(err, context.Canceled) || requests > 2 {
		t.Fatal("fallback ignored cancellation", requests, err)
	}
}

func TestCheckDoesNotFallbackForOtherAPIErrors(t *testing.T) {
	for _, code := range []int{http.StatusOK, http.StatusNotFound, http.StatusInternalServerError} {
		t.Run(http.StatusText(code), func(t *testing.T) {
			client := New("")
			requests := 0
			client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
				requests++
				return &http.Response{StatusCode: code, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("invalid JSON")), Request: r}, nil
			})
			_, err := client.Check(context.Background())
			if err == nil || requests != 1 || (errors.Is(err, ErrNoRelease) != (code == http.StatusNotFound)) {
				t.Fatal("API error triggered fallback or incorrect no-release state", requests, err)
			}
		})
	}
	client := New("")
	client.API = "https://api.github.com/repos/other/repo/releases/latest"
	requests := 0
	client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
		requests++
		return &http.Response{StatusCode: 403, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("")), Request: r}, nil
	})
	if _, err := client.Check(context.Background()); err == nil || requests != 1 {
		t.Fatal("custom repository unexpectedly used default fallback", requests, err)
	}
}

func TestFallbackRetainsDownloadRedirectPolicy(t *testing.T) {
	client := New("")
	requests := 0
	client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
		requests++
		status, header := 403, make(http.Header)
		switch r.URL.String() {
		case releaseRepository + "/latest":
			status = 302
			header.Set("Location", releaseRepository+"/tag/v0.2.0")
		case releaseRepository + "/download/v0.2.0/manifest.json":
			status = 302
			header.Set("Location", "https://evil.test/manifest.json")
		}
		return &http.Response{StatusCode: status, Header: header, Body: io.NopCloser(strings.NewReader("")), Request: r}, nil
	})
	if _, err := client.Check(context.Background()); err == nil || requests != 3 || !strings.Contains(err.Error(), "untrusted release host") {
		t.Fatal("fallback changed the download redirect policy", requests, err)
	}
}
