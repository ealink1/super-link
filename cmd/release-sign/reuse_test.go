package main

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/ealink1/super-link/internal/infra/release"
)

func TestSigningReusesOnlyAuthenticatedExactDrivers(t *testing.T) {
	pub, private, _ := ed25519.GenerateKey(rand.Reader)
	key := base64.StdEncoding.EncodeToString(pub)
	t.Setenv("SUPERLINK_RELEASE_PRIVATE_KEY", base64.StdEncoding.EncodeToString(private))
	t.Setenv("SUPERLINK_RELEASE_PUBLIC_KEY", key)
	directory := t.TempDir()
	hash := func(raw []byte) string { sum := sha256.Sum256(raw); return hex.EncodeToString(sum[:]) }
	write := func(name string, raw []byte) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(directory, name), raw, 0600); err != nil {
			t.Fatal(err)
		}
	}
	writeJSON := func(name string, value any) { raw, _ := json.Marshal(value); write(name, raw) }
	driver := release.Artifact{ID: "sqlite", Kind: "driver", OS: "darwin", Arch: "arm64", Filename: "sqlite-agent_0.1.0_darwin_arm64", URL: "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/v0.1.0/sqlite-agent_0.1.0_darwin_arm64", Size: 123, SHA256: hash([]byte("old driver")), Revision: "reviewed", Protocol: "json-lines-v2"}
	base := release.Manifest{Schema: 1, Version: "0.1.0", Channel: "stable", PublishedAt: time.Now().UTC(), Artifacts: []release.Artifact{driver}}
	raw, _ := json.Marshal(base)
	sig := []byte(base64.StdEncoding.EncodeToString(ed25519.Sign(private, raw)))
	proof := reuseProof{Schema: 1, BaseVersion: "0.1.0", DriverVersion: "0.1.0", BaseSource: strings.Repeat("a", 40), Source: strings.Repeat("b", 40), TargetVersion: "0.2.0", PublicKey: key, ManifestSHA256: hash(raw), SignatureSHA256: hash(sig)}
	write("driver-base-manifest.json", raw)
	write("driver-base-manifest.json.sig", sig)
	writeJSON("driver-reuse.json", proof)
	contents := []byte("new application")
	app := release.Artifact{ID: "superlink", Kind: "app", OS: "darwin", Arch: "arm64", Filename: "app.zip", URL: "https://github.com/ealink1/super-link/releases/download/v0.2.0/app.zip", Size: int64(len(contents)), SHA256: hash(contents)}
	write("app.zip", contents)
	assets := []release.Artifact{app, driver}
	writeJSON("assets-darwin-arm64.json", assets)
	if err := sign(directory, "0.2.0", "stable", ""); err != nil {
		t.Fatal(err)
	}
	signed, _ := os.ReadFile(filepath.Join(directory, "manifest.json"))
	signature, _ := os.ReadFile(filepath.Join(directory, "manifest.json.sig"))
	verified, err := release.Verify(signed, signature, key)
	if err != nil || verified.Artifacts[1] != driver {
		t.Fatal("previous immutable driver not retained", err)
	}
	for _, test := range []struct {
		name    string
		mutate  func()
		restore func()
	}{
		{"changed driver", func() { assets[1].Revision = "forged"; writeJSON("assets-darwin-arm64.json", assets) }, func() { assets[1] = driver; writeJSON("assets-darwin-arm64.json", assets) }},
		{"missing driver", func() { writeJSON("assets-darwin-arm64.json", assets[:1]) }, func() { writeJSON("assets-darwin-arm64.json", assets) }},
		{"tampered signature", func() { write("driver-base-manifest.json.sig", []byte("invalid")) }, func() { write("driver-base-manifest.json.sig", sig) }},
		{"forged base with matching proof hash", func() {
			forged := append([]byte(nil), raw...)
			forged[0] = ' '
			write("driver-base-manifest.json", forged)
			proof.ManifestSHA256 = hash(forged)
			writeJSON("driver-reuse.json", proof)
		}, func() {
			write("driver-base-manifest.json", raw)
			proof.ManifestSHA256 = hash(raw)
			writeJSON("driver-reuse.json", proof)
		}},
		{"corrupt new application", func() { write("app.zip", []byte("tampered")) }, func() { write("app.zip", contents) }},
		{"wrong trust key", func() {
			proof.PublicKey = base64.StdEncoding.EncodeToString(make([]byte, 32))
			writeJSON("driver-reuse.json", proof)
		}, func() { proof.PublicKey = key; writeJSON("driver-reuse.json", proof) }},
		{"incomplete proof", func() { os.Remove(filepath.Join(directory, "driver-reuse.json")) }, func() { writeJSON("driver-reuse.json", proof) }},
	} {
		t.Run(test.name, func(t *testing.T) {
			test.mutate()
			defer test.restore()
			if err := sign(directory, "0.2.0", "stable", ""); err == nil {
				t.Fatal("accepted untrusted reuse")
			}
		})
	}
}
