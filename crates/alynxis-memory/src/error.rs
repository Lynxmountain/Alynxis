//! Unified error type for `alynxis-memory`. Ordinary engineered
//! infrastructure (Philosophy 6's carve-out) — kept independent of the
//! other crates' error types, same pattern as `alynxis-worldmodel`.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
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

    #[error("episode not found: {0}")]
    EpisodeNotFound(String),

    #[error("procedural pattern not found: {0}")]
    ProceduralPatternNotFound(String),

    #[error("malformed stored data: {0}")]
    Malformed(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
