// verify-driver-reuse verifies public metadata; it never reads a signing key.
package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"

	"github.com/ealink1/super-link/internal/infra/release"
)

func readBounded(path string, limit int64) ([]byte, error) {
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

func main() {
	manifest := flag.String("manifest", "", "previous signed manifest")
	signature := flag.String("signature", "", "previous manifest signature")
	key := flag.String("public-key", "", "configured application trust key")
	baseVersion := flag.String("base-version", "", "expected previous application version")
	target := flag.String("target-version", "", "new application version")
	flag.Parse()
	raw, err := readBounded(*manifest, 1<<20)
	if err == nil {
		var sig []byte
		sig, err = readBounded(*signature, 1024)
		if err == nil {
			var base release.Manifest
			base, err = release.VerifyDriverBase(raw, sig, *key, *target)
			if err == nil && base.Version != *baseVersion {
				err = errors.New("driver base tag and signed manifest version differ")
			}
		}
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "driver reuse verification:", err)
		os.Exit(1)
	}
	fmt.Println("Previous stable driver manifest signature verified")
}
