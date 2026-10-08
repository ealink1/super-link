package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"regexp"

	"github.com/ealink1/super-link/internal/infra/release"
)

type reuseProof struct {
	Schema          int    `json:"schema"`
	BaseVersion     string `json:"base_app_version"`
	DriverVersion   string `json:"driver_version"`
	BaseSource      string `json:"base_source"`
	Source          string `json:"source"`
	TargetVersion   string `json:"target_version"`
	PublicKey       string `json:"public_key"`
	ManifestSHA256  string `json:"manifest_sha256"`
	SignatureSHA256 string `json:"signature_sha256"`
}

func readReuseFile(directory, name string, limit int64) ([]byte, error) {
	path := filepath.Join(directory, name)
	info, err := os.Lstat(path)
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() || info.Size() > limit {
		return nil, errors.New("invalid driver reuse metadata file")
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	raw, err := io.ReadAll(io.LimitReader(file, limit+1))
	if int64(len(raw)) > limit {
		return nil, errors.New("driver reuse metadata exceeds size limit")
	}
	return raw, err
}

func readDriverBase(directory, publicKey, version string) (*release.Manifest, error) {
	proofRaw, err := readReuseFile(directory, "driver-reuse.json", 4096)
	if errors.Is(err, os.ErrNotExist) {
		for _, name := range []string{"driver-base-manifest.json", "driver-base-manifest.json.sig"} {
			if _, statErr := os.Lstat(filepath.Join(directory, name)); !errors.Is(statErr, os.ErrNotExist) {
				return nil, errors.New("incomplete driver reuse proof")
			}
		}
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var proof reuseProof
	decoder := json.NewDecoder(bytes.NewReader(proofRaw))
	decoder.DisallowUnknownFields()
	if err = decoder.Decode(&proof); err != nil {
		return nil, err
	}
	shaPattern := regexp.MustCompile(`^[0-9a-f]{40}$`)
	if decoder.Decode(new(any)) != io.EOF || proof.Schema != 1 || proof.TargetVersion != version || proof.PublicKey != publicKey || !shaPattern.MatchString(proof.Source) || !shaPattern.MatchString(proof.BaseSource) {
		return nil, errors.New("driver reuse proof identity mismatch")
	}
	raw, err := readReuseFile(directory, "driver-base-manifest.json", 1<<20)
	if err != nil {
		return nil, err
	}
	sig, err := readReuseFile(directory, "driver-base-manifest.json.sig", 1024)
	if err != nil {
		return nil, err
	}
	manifestHash, signatureHash := sha256.Sum256(raw), sha256.Sum256(sig)
	if proof.ManifestSHA256 != hex.EncodeToString(manifestHash[:]) || proof.SignatureSHA256 != hex.EncodeToString(signatureHash[:]) {
		return nil, errors.New("driver reuse proof digest mismatch")
	}
	base, err := release.VerifyDriverBase(raw, sig, publicKey, version)
	if err != nil {
		return nil, err
	}
	if base.Version != proof.BaseVersion {
		return nil, errors.New("driver base version mismatch")
	}
	for _, artifact := range base.Artifacts {
		if artifact.Kind == "driver" && artifact.URL != "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/v"+proof.DriverVersion+"/"+artifact.Filename {
			return nil, errors.New("driver base repository/version mismatch")
		}
	}
	return &base, nil
}
