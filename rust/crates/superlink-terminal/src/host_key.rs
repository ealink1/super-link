//! Host key trust policy.
//!
//! The first connection to an unknown server never succeeds silently: the
//! caller receives the observed fingerprint and retries with an explicit
//! decision. A pinned fingerprint that no longer matches is always fatal.

use crate::error::{Result, TerminalError};
use crate::fingerprint::fingerprints_match;

/// What to do with the server identity presented during the handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Accept only this fingerprint. An empty pin behaves like [Self::Ask].
    Pinned(String),
    /// Report the observed fingerprint and refuse the connection.
    Ask,
    /// Trust this exact fingerprint once, without asking to persist it.
    TrustOnce(String),
    /// Trust this exact fingerprint and report that it should be stored.
    TrustAndRemember(String),
}

/// Outcome of a host key decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostKeyDecision {
    /// The caller should persist this fingerprint.
    pub remember: bool,
}

impl HostKeyPolicy {
    /// Decides whether the observed fingerprint may be trusted.
    ///
    /// A retry after user confirmation still checks the explicit fingerprint,
    /// so it cannot silently accept a different key.
    pub fn decide(&self, observed: &str) -> Result<HostKeyDecision> {
        match self {
            Self::Pinned(expected) if expected.trim().is_empty() => {
                Err(TerminalError::HostKeyUnknown {
                    fingerprint: observed.to_owned(),
                })
            }
            Self::Pinned(expected) => {
                if fingerprints_match(expected, observed) {
                    Ok(HostKeyDecision { remember: false })
                } else {
                    Err(changed(expected, observed))
                }
            }
            Self::Ask => Err(TerminalError::HostKeyUnknown {
                fingerprint: observed.to_owned(),
            }),
            Self::TrustOnce(expected) => {
                if fingerprints_match(expected, observed) {
                    Ok(HostKeyDecision { remember: false })
                } else {
                    Err(changed(expected, observed))
                }
            }
            Self::TrustAndRemember(expected) => {
                if fingerprints_match(expected, observed) {
                    Ok(HostKeyDecision { remember: true })
                } else {
                    Err(changed(expected, observed))
                }
            }
        }
    }
}

fn changed(expected: &str, observed: &str) -> TerminalError {
    TerminalError::HostKeyChanged {
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: &str = "SHA256:eLQy6UcSXjIOFdKRqsuK49h6xrNJsFR4cM8QE9TtAxA";

    #[test]
    fn unknown_servers_are_reported_not_trusted() {
        let error = HostKeyPolicy::Ask.decide(PIN).unwrap_err();
        assert!(matches!(
            error,
            TerminalError::HostKeyUnknown { fingerprint } if fingerprint == PIN
        ));
        assert!(matches!(
            HostKeyPolicy::Pinned(String::new()).decide(PIN),
            Err(TerminalError::HostKeyUnknown { .. })
        ));
    }

    #[test]
    fn pinned_and_explicit_fingerprints_must_match() {
        assert_eq!(
            HostKeyPolicy::Pinned(PIN.into()).decide(PIN).unwrap(),
            HostKeyDecision { remember: false }
        );
        assert!(
            HostKeyPolicy::Pinned(PIN.into())
                .decide("SHA256:other")
                .is_err()
        );
        assert!(
            HostKeyPolicy::TrustOnce(PIN.into())
                .decide("SHA256:other")
                .is_err()
        );
        assert!(
            HostKeyPolicy::TrustOnce("SHA256:other".into())
                .decide(PIN)
                .is_err()
        );
    }

    #[test]
    fn one_time_trust_does_not_request_persistence() {
        assert_eq!(
            HostKeyPolicy::TrustOnce(PIN.into()).decide(PIN).unwrap(),
            HostKeyDecision { remember: false }
        );
        assert_eq!(
            HostKeyPolicy::TrustAndRemember(PIN.into())
                .decide(PIN)
                .unwrap(),
            HostKeyDecision { remember: true }
        );
    }

    #[test]
    fn changed_keys_are_fatal() {
        let error = HostKeyPolicy::Pinned(PIN.into())
            .decide("SHA256:changed")
            .unwrap_err();
        match error {
            TerminalError::HostKeyChanged { expected, observed } => {
                assert_eq!(expected, PIN);
                assert_eq!(observed, "SHA256:changed");
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
