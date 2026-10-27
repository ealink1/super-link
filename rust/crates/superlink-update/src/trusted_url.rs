//! Allow list for release download addresses.

use url::Url;

use crate::error::{Result, UpdateError};

const TRUSTED_HOSTS: [&str; 4] = [
    "github.com",
    "api.github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

const REPOSITORY_PATHS: [&str; 2] = [
    "/ealink1/super-link/releases/download/",
    "/ealink1/SuperLink-DriverAgents/releases/download/",
];

/// Parses a release URL and rejects anything outside the trusted origins.
///
/// Only credential-free HTTPS on a known GitHub host is accepted; `github.com`
/// downloads must additionally live under a configured repository.
pub fn trusted_url(text: &str) -> Result<Url> {
    let url = Url::parse(text).map_err(|error| UpdateError::UntrustedUrl(error.to_string()))?;
    if url.scheme() != "https" {
        return Err(UpdateError::UntrustedUrl("scheme must be https".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UpdateError::UntrustedUrl(
            "credentials are not allowed".into(),
        ));
    }
    if url.fragment().is_some() {
        return Err(UpdateError::UntrustedUrl(
            "fragments are not allowed".into(),
        ));
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !TRUSTED_HOSTS.contains(&host.as_str()) {
        return Err(UpdateError::UntrustedUrl(format!(
            "untrusted host {host:?}"
        )));
    }
    if !matches!(url.port(), None | Some(443)) {
        return Err(UpdateError::UntrustedUrl("port must be 443".into()));
    }
    if host == "github.com"
        && !REPOSITORY_PATHS
            .iter()
            .any(|prefix| url.path().starts_with(prefix))
    {
        return Err(UpdateError::UntrustedUrl(
            "artifact is outside the configured repository".into(),
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::trusted_url;

    #[test]
    fn accepts_repository_downloads() {
        assert!(
            trusted_url(
                "https://github.com/ealink1/super-link/releases/download/v0.2.0/SuperLink.zip"
            )
            .is_ok()
        );
        assert!(
            trusted_url("https://api.github.com/repos/ealink1/super-link/releases/latest").is_ok()
        );
    }

    #[test]
    fn rejects_untrusted_addresses() {
        for url in [
            "http://github.com/ealink1/super-link/releases/download/v1/app.zip",
            "https://user:pass@github.com/ealink1/super-link/releases/download/v1/app.zip",
            "https://example.com/ealink1/super-link/releases/download/v1/app.zip",
            "https://github.com/other/repo/releases/download/v1/app.zip",
            "https://github.com/ealink1/super-link/releases/download/v1/app.zip#frag",
            "https://github.com:8443/ealink1/super-link/releases/download/v1/app.zip",
            "not a url",
        ] {
            assert!(trusted_url(url).is_err(), "{url} should be rejected");
        }
    }
}
