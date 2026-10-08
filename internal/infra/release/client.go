package release

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ealink1/super-link/internal/upstream/appdata"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

var ErrNoRelease = errors.New("repository has no published release")

const latestAPI = "https://api.github.com/repos/ealink1/super-link/releases/latest"

type Client struct {
	HTTP      *http.Client
	API       string
	PublicKey string
}
type releaseInfo struct {
	Tag        string `json:"tag_name"`
	Draft      bool   `json:"draft"`
	Prerelease bool   `json:"prerelease"`
	Assets     []struct {
		Name string `json:"name"`
		URL  string `json:"browser_download_url"`
	} `json:"assets"`
}

func New(key string) *Client {
	return &Client{PublicKey: key, API: latestAPI, HTTP: &http.Client{Timeout: 60 * time.Second, CheckRedirect: func(req *http.Request, via []*http.Request) error {
		if len(via) > 5 {
			return errors.New("too many release redirects")
		}
		_, err := TrustedURL(req.URL.String())
		return err
	}}}
}
func (c *Client) get(ctx context.Context, address string, limit int64) ([]byte, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, address, nil)
	if err != nil {
		return nil, err
	}
	request.Header.Set("Accept", "application/vnd.github+json")
	request.Header.Set("User-Agent", "SuperLink")
	response, err := c.HTTP.Do(request)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return nil, responseError(response)
	}
	raw, err := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(raw)) > limit {
		return nil, errors.New("release metadata exceeds size limit")
	}
	return raw, nil
}
func (c *Client) Check(ctx context.Context) (Manifest, error) {
	raw, err := c.get(ctx, c.API, 1<<20)
	if err != nil {
		var status *HTTPError
		if errors.As(err, &status) {
			if status.StatusCode == http.StatusNotFound {
				return Manifest{}, ErrNoRelease
			}
			if c.API == latestAPI && (status.StatusCode == http.StatusForbidden || status.StatusCode == http.StatusTooManyRequests) {
				manifest, fallbackErr := c.checkStableDownload(ctx)
				if fallbackErr == nil || errors.Is(fallbackErr, ErrNoRelease) {
					return manifest, fallbackErr
				}
				return Manifest{}, fmt.Errorf("%w; GitHub stable fallback failed: %w", err, fallbackErr)
			}
		}
		return Manifest{}, err
	}
	var info releaseInfo
	if err = json.Unmarshal(raw, &info); err != nil {
		return Manifest{}, err
	}
	if info.Draft || info.Prerelease {
		return Manifest{}, errors.New("latest stable release cannot be draft or prerelease")
	}
	var manifestURL, signatureURL string
	for _, asset := range info.Assets {
		switch asset.Name {
		case "manifest.json":
			manifestURL = asset.URL
		case "manifest.json.sig":
			signatureURL = asset.URL
		}
	}
	if manifestURL == "" || signatureURL == "" {
		return Manifest{}, errors.New("release has no signed manifest")
	}
	return c.checkManifest(ctx, info.Tag, manifestURL, signatureURL)
}

func (c *Client) checkManifest(ctx context.Context, tag, manifestURL, signatureURL string) (Manifest, error) {
	for _, address := range []string{manifestURL, signatureURL} {
		if _, err := TrustedURL(address); err != nil {
			return Manifest{}, err
		}
	}
	raw, err := c.get(ctx, manifestURL, 1<<20)
	if err != nil {
		return Manifest{}, err
	}
	signature, err := c.get(ctx, signatureURL, 1024)
	if err != nil {
		return Manifest{}, err
	}
	manifest, err := Verify(raw, signature, c.PublicKey)
	if err != nil {
		return Manifest{}, err
	}
	if strings.TrimPrefix(tag, "v") != strings.TrimPrefix(manifest.Version, "v") {
		return Manifest{}, errors.New("release tag and signed manifest version differ")
	}
	if manifest.Channel != "stable" {
		return Manifest{}, errors.New("latest stable release has a preview manifest")
	}
	return manifest, nil
}

// Download writes a private temporary file and only returns after exact size and
// SHA256 verification. Interrupted or invalid downloads are removed.
func (c *Client) Download(ctx context.Context, artifact Artifact, directory string) (string, error) {
	return c.DownloadWithProgress(ctx, artifact, directory, nil)
}

// DownloadWithProgress reports bytes written synchronously on the download worker.
// Callbacks must return promptly; total comes from the signed manifest.
func (c *Client) DownloadWithProgress(ctx context.Context, artifact Artifact, directory string, progress func(downloaded, total int64)) (string, error) {
	if _, err := TrustedURL(artifact.URL); err != nil {
		return "", err
	}
	if artifact.Size <= 0 || artifact.Size > MaxArtifactBytes {
		return "", errors.New("invalid download size")
	}
	if !filenamePattern.MatchString(artifact.Filename) {
		return "", errors.New("invalid filename")
	}
	if err := os.MkdirAll(directory, 0700); err != nil {
		return "", err
	}
	file, err := os.CreateTemp(directory, ".download-*")
	if err != nil {
		return "", err
	}
	path := file.Name()
	complete := false
	defer func() {
		_ = file.Close()
		if !complete {
			_ = os.Remove(path)
		}
	}()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, artifact.URL, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("User-Agent", "SuperLink")
	response, err := c.HTTP.Do(req)
	if err != nil {
		return "", err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return "", fmt.Errorf("download HTTP %d", response.StatusCode)
	}
	if response.ContentLength > artifact.Size {
		return "", errors.New("download exceeds declared size")
	}
	hash := sha256.New()
	writer := io.Writer(io.MultiWriter(file, hash))
	if progress != nil {
		progress(0, artifact.Size)
		writer = &progressWriter{writer: writer, total: artifact.Size, report: progress}
	}
	count, err := io.Copy(writer, io.LimitReader(response.Body, artifact.Size+1))
	if err != nil {
		return "", err
	}
	if count != artifact.Size || !strings.EqualFold(hex.EncodeToString(hash.Sum(nil)), artifact.SHA256) {
		return "", errors.New("download size or SHA256 mismatch")
	}
	if err = file.Sync(); err != nil {
		return "", err
	}
	if err = file.Close(); err != nil {
		return "", err
	}
	target := filepath.Join(directory, artifact.Filename)
	if err = appdata.AtomicReplaceFile(path, target); err != nil {
		return "", err
	}
	complete = true
	return target, nil
}

// progressWriter reports successful writes without buffering the package in memory.
type progressWriter struct {
	writer            io.Writer
	downloaded, total int64
	report            func(int64, int64)
}

func (w *progressWriter) Write(data []byte) (int, error) {
	n, err := w.writer.Write(data)
	w.downloaded += int64(n)
	w.report(min(w.downloaded, w.total), w.total)
	return n, err
}
