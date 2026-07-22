//! Unified error type for `alynxis-values`. Ordinary engineered
//! infrastructure (Philosophy 6's carve-out).

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ValuesError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON (de)serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("unknown value kind")]
    UnknownValue,

    #[error("this operation requires a currently-valid, authenticated admin session")]
    AdminAuthRequired,
}

pub type Result<T> = std::result::Result<T, ValuesError>;
