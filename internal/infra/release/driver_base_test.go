package release

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"testing"
	"time"
)

func TestDriverBaseRequiresEarlierSignedStableRelease(t *testing.T) {
	public, private, _ := ed25519.GenerateKey(rand.Reader)
	key := base64.StdEncoding.EncodeToString(public)
	sum := sha256.Sum256([]byte("driver"))
	driver := Artifact{ID: "sqlite", Kind: "driver", OS: "linux", Arch: "amd64", Filename: "driver", URL: "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/v0.1.0/driver", SHA256: hex.EncodeToString(sum[:]), Size: 6, Revision: "reviewed", Protocol: "json-lines-v2"}
	base := Manifest{Schema: 1, Version: "0.1.0", Channel: "stable", PublishedAt: time.Now().UTC(), Artifacts: []Artifact{driver}}
	check := func(base Manifest, target string, valid bool) {
		t.Helper()
		raw, _ := json.Marshal(base)
		sig := []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw)))
		_, err := VerifyDriverBase(raw, sig, key, target)
		if (err == nil) != valid {
			t.Fatal("unexpected driver base acceptance", err)
		}
	}
	check(base, "0.2.0", true)
	check(base, "0.1.0", false)
	check(base, "0.0.9", false)
	base.Channel = "preview"
	check(base, "0.2.0", false)
	base.Channel = "stable"
	base.Version = "0.1.0-rc.1"
	check(base, "0.2.0", false)
	base.Version = "0.1.0"
	base.Artifacts[0].URL = "https://github.com/other/repo/releases/download/v0.1.0/driver"
	check(base, "0.2.0", false)
}
