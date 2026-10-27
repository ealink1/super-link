//! Release platform identifiers, matching Go's `runtime.GOOS`/`GOARCH` strings.

use serde::{Deserialize, Serialize};

/// Operating system names used by release manifests and package markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Darwin,
    Windows,
    Linux,
}

impl Os {
    /// Returns the wire name, e.g. `darwin`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Darwin => "darwin",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }

    /// Parses a wire name.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "darwin" => Some(Self::Darwin),
            "windows" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            _ => None,
        }
    }

    /// The operating system this build runs on, if releases support it.
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Darwin)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else {
            None
        }
    }
}

/// CPU architecture names used by release manifests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    Amd64,
    Arm64,
}

impl Arch {
    /// Returns the wire name, e.g. `arm64`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
        }
    }

    /// Parses a wire name.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "amd64" => Some(Self::Amd64),
            "arm64" => Some(Self::Arm64),
            _ => None,
        }
    }

    /// The architecture this build runs on, if releases support it.
    pub const fn current() -> Option<Self> {
        if cfg!(target_arch = "x86_64") {
            Some(Self::Amd64)
        } else if cfg!(target_arch = "aarch64") {
            Some(Self::Arm64)
        } else {
            None
        }
    }
}

/// An operating system and architecture pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    /// Creates a platform pair.
    pub const fn new(os: Os, arch: Arch) -> Self {
        Self { os, arch }
    }

    /// The current process platform, if releases support it.
    pub const fn current() -> Option<Self> {
        match (Os::current(), Arch::current()) {
            (Some(os), Some(arch)) => Some(Self { os, arch }),
            _ => None,
        }
    }

    /// Reports whether a manifest entry targets this platform.
    pub fn matches(&self, os: Os, arch: Arch) -> bool {
        self.os == os && self.arch == arch
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.os.as_str(), self.arch.as_str())
    }
}
