//! Unified error type for the alynxis-core crate.
//!
//! This is ordinary engineered infrastructure (Philosophy 6's carve-out for
//! "authentication, safety gates, data structures, control flow") — not
//! learned content — so a fixed enum here is appropriate and does not
//! conflict with the project's anti-hardcoding philosophy. Section 7a's
//! resolution against a `NodeKind`-style enum concerns learned graph
//! structure specifically, not error plumbing.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AlynxisError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config file at {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to serialize config: {source}")]
    ConfigSerialize {
        #[source]
        source: toml::ser::Error,
    },

    #[error(
        "Zone A integrity check FAILED for {file}: expected SHA-256 {expected}, found {actual}. \
         Refusing to boot — Zone A source has been modified since this binary was compiled. \
         If this change was intentional and reviewed, rebuild the binary to re-baseline the hash."
    )]
    ZoneIntegrityFailure {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("Zone A source file listed in the build-time manifest is missing at runtime: {path}")]
    ZoneFileMissing { path: PathBuf },

    #[error("admin credential error: {0}")]
    AdminCredential(String),

    #[error("admin session is not currently authenticated or has expired")]
    AdminSessionInvalid,

    #[error("refused: path {path} is registered in Zone A and cannot be modified by this pathway")]
    ZoneAWriteRefused { path: PathBuf },

    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AlynxisError>;
