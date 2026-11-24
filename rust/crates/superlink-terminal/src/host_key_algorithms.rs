//! Fyne's Go SSH client preferred ECDSA before Ed25519. Keep that order and
//! probe other strong key types when a saved pin belongs to another key type.
use russh::{
    Preferred,
    keys::{
        HashAlg,
        ssh_key::{Algorithm, EcdsaCurve},
    },
};
use std::borrow::Cow;

pub(crate) fn preferences() -> Vec<Preferred> {
    let keys = vec![
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Ed25519,
    ];
    let mut initial = keys.clone();
    // Preserve russh's existing compatibility fallback; alternate-pin probes
    // below never add algorithms outside the strong set above.
    initial.push(Algorithm::Rsa { hash: None });
    let mut attempts = vec![Preferred {
        key: Cow::Owned(initial),
        ..Preferred::DEFAULT
    }];
    attempts.extend(keys.into_iter().skip(1).map(|key| Preferred {
        key: Cow::Owned(vec![key]),
        ..Preferred::DEFAULT
    }));
    attempts
}
