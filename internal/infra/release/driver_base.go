package release

import (
	"errors"
	"strings"

	"golang.org/x/mod/semver"
)

// VerifyDriverBase authenticates a previous stable manifest before its exact
// driver records can be carried into a newer application manifest.
func VerifyDriverBase(raw, signature []byte, key, target string) (Manifest, error) {
	base, err := Verify(raw, signature, key)
	if err != nil {
		return base, err
	}
	if base.Channel != "stable" || semver.Prerelease("v"+strings.TrimPrefix(base.Version, "v")) != "" || !Newer(target, base.Version) {
		return base, errors.New("driver base must be an earlier stable application release")
	}
	drivers := 0
	for _, artifact := range base.Artifacts {
		if artifact.Kind == "driver" {
			drivers++
			if !strings.HasPrefix(artifact.URL, "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/") {
				return base, errors.New("reused driver must belong to the driver repository")
			}
		}
	}
	if drivers == 0 {
		return base, errors.New("signed driver base has no drivers")
	}
	return base, nil
}
