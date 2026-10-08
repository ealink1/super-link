// Package release verifies artifacts before they can become executable code.
package release

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/url"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"golang.org/x/mod/semver"
)

const MaxArtifactBytes int64 = 1 << 30

var filenamePattern = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,160}$`)

type Artifact struct {
	ID       string `json:"id"`
	Kind     string `json:"kind"`
	OS       string `json:"os"`
	Arch     string `json:"arch"`
	Filename string `json:"filename"`
	URL      string `json:"url"`
	Size     int64  `json:"size"`
	SHA256   string `json:"sha256"`
	Revision string `json:"revision,omitempty"`
	Protocol string `json:"protocol,omitempty"`
}
type Manifest struct {
	Schema      int        `json:"schema"`
	Version     string     `json:"version"`
	Channel     string     `json:"channel"`
	PublishedAt time.Time  `json:"publishedAt"`
	Artifacts   []Artifact `json:"artifacts"`
}

func Verify(raw, signature []byte, keyText string) (Manifest, error) {
	var manifest Manifest
	key, err := base64.StdEncoding.DecodeString(strings.TrimSpace(keyText))
	if err != nil || len(key) != ed25519.PublicKeySize {
		return manifest, errors.New("release signing public key is not configured")
	}
	decoded, err := base64.StdEncoding.DecodeString(strings.TrimSpace(string(signature)))
	if err != nil || !ed25519.Verify(ed25519.PublicKey(key), raw, decoded) {
		return manifest, errors.New("release manifest signature invalid")
	}
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	decoder.DisallowUnknownFields()
	if err = decoder.Decode(&manifest); err != nil {
		return manifest, err
	}
	if decoder.Decode(new(any)) != io.EOF {
		return manifest, errors.New("trailing manifest data")
	}
	if err = manifest.Validate(); err != nil {
		return manifest, err
	}
	return manifest, nil
}
func (m Manifest) Validate() error {
	if m.Schema != 1 {
		return errors.New("unsupported release manifest schema")
	}
	if !semver.IsValid("v" + strings.TrimPrefix(m.Version, "v")) {
		return errors.New("invalid release version")
	}
	if m.Channel != "stable" && m.Channel != "preview" {
		return errors.New("invalid release channel")
	}
	if len(m.Artifacts) == 0 || len(m.Artifacts) > 200 {
		return errors.New("invalid artifact count")
	}
	seen := map[string]bool{}
	for _, a := range m.Artifacts {
		if !filenamePattern.MatchString(a.ID) || !filenamePattern.MatchString(a.Filename) || filepath.Base(a.Filename) != a.Filename {
			return errors.New("invalid artifact identifier or filename")
		}
		if a.Kind != "app" && a.Kind != "driver" {
			return errors.New("unsupported artifact kind")
		}
		if a.Size <= 0 || a.Size > MaxArtifactBytes {
			return errors.New("invalid artifact size")
		}
		hash, err := hex.DecodeString(a.SHA256)
		if err != nil || len(hash) != 32 {
			return errors.New("invalid artifact SHA256")
		}
		if a.OS != "darwin" && a.OS != "windows" && a.OS != "linux" {
			return errors.New("unsupported operating system")
		}
		if a.Arch != "amd64" && a.Arch != "arm64" {
			return errors.New("unsupported architecture")
		}
		key := a.Kind + "/" + a.ID + "/" + a.OS + "/" + a.Arch
		if seen[key] {
			return fmt.Errorf("duplicate artifact %s", key)
		}
		seen[key] = true
		if a.Kind == "driver" && (a.Revision == "" || a.Protocol != "json-lines-v2") {
			return errors.New("driver compatibility metadata missing")
		}
		if _, err = TrustedURL(a.URL); err != nil {
			return err
		}
	}
	return nil
}
func TrustedURL(text string) (*url.URL, error) {
	u, err := url.Parse(text)
	if err != nil {
		return nil, err
	}
	if u.Scheme != "https" || u.User != nil || u.Fragment != "" {
		return nil, errors.New("artifact URL must use credential-free HTTPS")
	}
	host := strings.ToLower(u.Hostname())
	if host != "github.com" && host != "api.github.com" && host != "release-assets.githubusercontent.com" && host != "objects.githubusercontent.com" {
		return nil, errors.New("untrusted release host")
	}
	if u.Port() != "" && u.Port() != "443" {
		return nil, errors.New("invalid release port")
	}
	if host == "github.com" && !strings.HasPrefix(u.EscapedPath(), "/ealink1/super-link/releases/download/") &&
		!strings.HasPrefix(u.EscapedPath(), "/ealink1/SuperLink-DriverAgents/releases/download/") {
		return nil, errors.New("artifact is outside the configured repository")
	}
	return u, nil
}
func (m Manifest) Artifact(kind, id, goos, arch string) (Artifact, error) {
	for _, a := range m.Artifacts {
		if a.Kind == kind && a.ID == id && a.OS == goos && a.Arch == arch {
			return a, nil
		}
	}
	return Artifact{}, errors.New("this release has no matching platform artifact")
}
func Newer(candidate, current string) bool {
	return semver.Compare("v"+strings.TrimPrefix(candidate, "v"), "v"+strings.TrimPrefix(current, "v")) > 0
}
