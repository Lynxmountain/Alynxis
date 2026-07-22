//! Unified error type for `alynxis-worldmodel`. Ordinary engineered
//! infrastructure (Philosophy 6's carve-out) — kept independent of
//! `alynxis_core::AlynxisError` so this crate doesn't force an awkward
//! coupling on `alynxis-core`; conversions happen at call sites in
//! `alynxis-bin` if/when they're needed.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum WorldModelError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON (de)serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("node not found: {0}")]
    NodeNotFound(String),

    #[error("edge not found: {0}")]
    EdgeNotFound(String),

    #[error("invalid spatial position: {0}")]
    InvalidPosition(String),

    #[error("ingestion error: {0}")]
    Ingestion(String),
}

pub type Result<T> = std::result::Result<T, WorldModelError>;
