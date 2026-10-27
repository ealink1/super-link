//! Minimal semantic version parsing and ordering.
//!
//! Mirrors `golang.org/x/mod/semver` closely enough for release manifests:
//! an optional leading `v`, one to three numeric components, an optional
//! prerelease and ignored build metadata.

use std::cmp::Ordering;

const MAX_COMPONENT_DIGITS: usize = 18;
const MAX_TEXT_BYTES: usize = 128;

/// A parsed semantic version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<String>,
}

impl Version {
    /// Parses `v?MAJOR(.MINOR(.PATCH)?)?(-PRERELEASE)?(+BUILD)?`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return None;
        }
        let text = text.split('+').next()?;
        let (core, prerelease) = match text.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (text, None),
        };
        let mut components = core.split('.');
        let major = parse_number(components.next()?)?;
        let minor = match components.next() {
            Some(text) => parse_number(text)?,
            None => 0,
        };
        let patch = match components.next() {
            Some(text) => parse_number(text)?,
            None => 0,
        };
        if components.next().is_some() {
            return None;
        }
        if let Some(prerelease) = prerelease
            && !is_prerelease(prerelease)
        {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            prerelease: prerelease.map(str::to_owned),
        })
    }

    /// Returns the version without a leading `v`.
    pub fn to_string_without_prefix(&self) -> String {
        let mut text = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if let Some(prerelease) = &self.prerelease {
            text.push('-');
            text.push_str(prerelease);
        }
        text
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| {
                compare_prerelease(self.prerelease.as_deref(), other.prerelease.as_deref())
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Reports whether `candidate` is a higher release than `current`.
///
/// Unparsable versions never count as newer, matching Go's compare-as-equal
/// behaviour for invalid input.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (Version::parse(candidate), Version::parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn parse_number(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > MAX_COMPONENT_DIGITS {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn is_prerelease(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn compare_prerelease(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => compare_identifiers(left, right),
    }
}

fn compare_identifiers(left: &str, right: &str) -> Ordering {
    let mut left = left.split('.');
    let mut right = right.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) => {
                let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => left.cmp(right),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Version, is_newer};

    #[test]
    fn parses_shortened_and_prefixed_versions() {
        assert_eq!(
            Version::parse("v1").unwrap().to_string_without_prefix(),
            "1.0.0"
        );
        assert_eq!(
            Version::parse("1.2").unwrap().to_string_without_prefix(),
            "1.2.0"
        );
        assert_eq!(
            Version::parse("v0.2.0-rc.1+build")
                .unwrap()
                .to_string_without_prefix(),
            "0.2.0-rc.1"
        );
    }

    #[test]
    fn rejects_invalid_versions() {
        for text in ["", "v", "1.2.3.4", "01.2.3", "1.x.3", "1.2.3-", "1.2.3 -x"] {
            assert!(Version::parse(text).is_none(), "{text} should be rejected");
        }
    }

    #[test]
    fn orders_releases_before_prereleases() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.2.0", "0.2.0-rc.1"));
        assert!(!is_newer("0.2.0-rc.2", "0.2.0-rc.10"));
        assert!(is_newer("0.2.0-rc.10", "0.2.0-rc.2"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
