// release-sign is a maintainer tool. It is never bundled with the application.
package main

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/ealink1/super-link/internal/infra/release"
)

func main() {
	assets := flag.String("assets-dir", "dist", "directory containing platform assets-*.json and binaries")
	version := flag.String("version", "", "release version")
	channel := flag.String("channel", "stable", "stable or preview")
	keyPath := flag.String("key-file", "", "base64 Ed25519 private key file; otherwise SUPERLINK_RELEASE_PRIVATE_KEY")
	generate := flag.String("generate-key-file", "", "create a new 0600 private key file without overwriting an existing key")
	flag.Parse()
	if *generate != "" {
		if err := generateKey(*generate); err != nil {
			fmt.Fprintln(os.Stderr, "release key generation:", err)
			os.Exit(1)
		}
		return
	}
	if err := sign(*assets, *version, *channel, *keyPath); err != nil {
		fmt.Fprintln(os.Stderr, "release signing:", err)
		os.Exit(1)
	}
}

func generateKey(path string) error {
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		return err
	}
	defer clear(private)
	file, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
	if err != nil {
		return err
	}
	_, writeErr := file.WriteString(base64.StdEncoding.EncodeToString(private) + "\n")
	if err = errors.Join(writeErr, file.Sync(), file.Close()); err != nil {
		return err
	}
	fmt.Println("Private key saved to the requested file. Public key:", base64.StdEncoding.EncodeToString(public))
	return nil
}
func sign(directory, version, channel, keyPath string) error {
	text := os.Getenv("SUPERLINK_RELEASE_PRIVATE_KEY")
	if keyPath != "" {
		raw, err := os.ReadFile(keyPath)
		if err != nil {
			return err
		}
		text = string(raw)
	}
	key, err := base64.StdEncoding.DecodeString(strings.TrimSpace(text))
	if err != nil || len(key) != ed25519.PrivateKeySize {
		return errors.New("provide a base64 64-byte Ed25519 private key")
	}
	defer clear(key)
	public := base64.StdEncoding.EncodeToString(ed25519.PrivateKey(key).Public().(ed25519.PublicKey))
	if expected := strings.TrimSpace(os.Getenv("SUPERLINK_RELEASE_PUBLIC_KEY")); expected != "" && public != expected {
		return errors.New("signing key does not match the application release public key")
	}
	base, err := readDriverBase(directory, public, strings.TrimPrefix(version, "v"))
	if err != nil {
		return err
	}
	files, err := filepath.Glob(filepath.Join(directory, "assets-*.json"))
	if err != nil {
		return err
	}
	manifest := release.Manifest{Schema: 1, Version: strings.TrimPrefix(version, "v"), Channel: channel, PublishedAt: time.Now().UTC()}
	for _, file := range files {
		raw, err := os.ReadFile(file)
		if err != nil {
			return err
		}
		var assets []release.Artifact
		if err = json.Unmarshal(raw, &assets); err != nil {
			return err
		}
		manifest.Artifacts = append(manifest.Artifacts, assets...)
	}
	if err = manifest.Validate(); err != nil {
		return err
	}
	reused := 0
	for _, asset := range manifest.Artifacts {
		if base != nil && asset.Kind == "driver" {
			original, err := base.Artifact("driver", asset.ID, asset.OS, asset.Arch)
			if err != nil || original != asset {
				return errors.New("reused driver differs from the previous signed manifest")
			}
			reused++
			continue
		}
		input, err := os.Open(filepath.Join(directory, asset.Filename))
		if err != nil {
			return err
		}
		hash := sha256.New()
		size, copyErr := io.Copy(hash, input)
		if err = errors.Join(copyErr, input.Close()); err != nil {
			return err
		}
		if size != asset.Size || !strings.EqualFold(hex.EncodeToString(hash.Sum(nil)), asset.SHA256) {
			return fmt.Errorf("asset differs from build metadata: %s", asset.Filename)
		}
		if !strings.Contains(asset.URL, "/releases/download/v"+manifest.Version+"/") {
			return errors.New("asset version differs from manifest")
		}
	}
	if base != nil {
		expected := 0
		for _, artifact := range base.Artifacts {
			if artifact.Kind == "driver" {
				expected++
			}
		}
		if reused != expected {
			return errors.New("reused driver inventory incomplete")
		}
	}
	raw, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return err
	}
	raw = append(raw, '\n')
	sig := base64.StdEncoding.EncodeToString(ed25519.Sign(ed25519.PrivateKey(key), raw)) + "\n"
	if _, err = release.Verify(raw, []byte(sig), public); err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(directory, "manifest.json"), raw, 0644); err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(directory, "manifest.json.sig"), []byte(sig), 0644); err != nil {
		return err
	}
	fmt.Println("Signed and verified manifest; public key:", public)
	return nil
}
