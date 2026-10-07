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

	"github.com/ealink1/super-link/internal/infra/release"
)

func TestSigningChecksActualAssetsAndProducesVerifiableManifest(t *testing.T) {
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("SUPERLINK_RELEASE_PRIVATE_KEY", base64.StdEncoding.EncodeToString(private))
	t.Setenv("SUPERLINK_RELEASE_PUBLIC_KEY", base64.StdEncoding.EncodeToString(public))
	directory := t.TempDir()
	contents := []byte("application fixture")
	sum := sha256.Sum256(contents)
	asset := release.Artifact{ID: "superlink", Kind: "app", OS: "darwin", Arch: "arm64", Filename: "app.zip", URL: "https://github.com/ealink1/super-link/releases/download/v0.2.0/app.zip", Size: int64(len(contents)), SHA256: hex.EncodeToString(sum[:])}
	raw, _ := json.Marshal([]release.Artifact{asset})
	if err = os.WriteFile(filepath.Join(directory, "assets-darwin-arm64.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(filepath.Join(directory, "app.zip"), contents, 0600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("SUPERLINK_RELEASE_PUBLIC_KEY", base64.StdEncoding.EncodeToString(make([]byte, ed25519.PublicKeySize)))
	if err = sign(directory, "0.2.0", "stable", ""); err == nil || !strings.Contains(err.Error(), "does not match") {
		t.Fatal("accepted a key different from application public key", err)
	}
	if _, err := os.Stat(filepath.Join(directory, "manifest.json")); !os.IsNotExist(err) {
		t.Fatal("published manifest despite key mismatch")
	}
	t.Setenv("SUPERLINK_RELEASE_PUBLIC_KEY", base64.StdEncoding.EncodeToString(public))
	if err = sign(directory, "0.2.0", "stable", ""); err != nil {
		t.Fatal(err)
	}
	manifest, _ := os.ReadFile(filepath.Join(directory, "manifest.json"))
	signature, _ := os.ReadFile(filepath.Join(directory, "manifest.json.sig"))
	verified, err := release.Verify(manifest, signature, base64.StdEncoding.EncodeToString(public))
	if err != nil || verified.Version != "0.2.0" || len(verified.Artifacts) != 1 {
		t.Fatal("signed release does not verify", err)
	}
	if err = os.WriteFile(filepath.Join(directory, "app.zip"), []byte("tampered"), 0600); err != nil {
		t.Fatal(err)
	}
	if err = sign(directory, "0.2.0", "stable", ""); err == nil {
		t.Fatal("signed an artifact different from the build metadata")
	}
}

func TestKeyGenerationDoesNotOverwriteExistingSigningIdentity(t *testing.T) {
	path := filepath.Join(t.TempDir(), "private.key")
	if err := generateKey(path); err != nil {
		t.Fatal(err)
	}
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	private, err := base64.StdEncoding.DecodeString(string(before[:len(before)-1]))
	if err != nil || len(private) != ed25519.PrivateKeySize {
		t.Fatal("generated key is not an Ed25519 signing key")
	}
	if err := generateKey(path); err == nil {
		t.Fatal("overwrote an existing private key")
	}
	after, _ := os.ReadFile(path)
	if string(before) != string(after) {
		t.Fatal("existing signing identity changed")
	}
}
