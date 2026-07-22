//! Config (Part 1, Section 10: "Config (including the `require_zone_b_review`
//! flag, default true — Section 9a)").

use crate::error::{AlynxisError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

fn default_data_dir() -> PathBuf {
    resolve_default_data_dir()
}

fn default_admin_inactivity_timeout_secs() -> u64 {
    // 30 minutes, per Lynx's directive. Changeable later via an admin
    // panel (Part 12+) through AdminSession::set_inactivity_timeout, which
    // requires an already-authenticated session.
    1800
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Best-effort resolution of a per-user data directory without pulling in
/// the `dirs` crate as an extra dependency — falls back to a relative path
/// if `HOME` isn't set (e.g. some sandboxed/CI environments).
fn resolve_default_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".alynxis")
    } else {
        PathBuf::from("./alynxis_data")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Section 9a: Zone B changes (core reasoning/learning rules) are
    /// logged and queued for Lynx's review before taking effect. This flag
    /// gates that behavior; default true means review is required. This is
    /// intended to be changed only via a deliberate, manual config edit —
    /// not something Alynxis itself should ever be able to flip.
    #[serde(default = "default_true")]
    pub require_zone_b_review: bool,

    /// Root directory for all persisted Alynxis state: config, admin
    /// credential store, admin identity, logs, and (from Part 2 onward)
    /// the WorldModel's SQLite database.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// How long an authenticated admin session remains valid with no
    /// recorded activity. Changeable at runtime only by an already-
    /// authenticated admin session (see `core::admin::AdminSession::set_inactivity_timeout`).
    /// The session also ends immediately, independent of this timeout, when
    /// explicitly ended — the intended future call site being a sleep-tier
    /// transition once Parts 4a/19 exist.
    #[serde(default = "default_admin_inactivity_timeout_secs")]
    pub admin_inactivity_timeout_secs: u64,

    /// "trace" | "debug" | "info" | "warn" | "error" — passed to the
    /// tracing-subscriber env-filter at startup.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            require_zone_b_review: default_true(),
            data_dir: default_data_dir(),
            admin_inactivity_timeout_secs: default_admin_inactivity_timeout_secs(),
            log_level: default_log_level(),
        }
    }
}

impl Config {
    /// Loads config from `path` if it exists; otherwise writes out a fresh
    /// default config file at `path` (so the file becomes the visible,
    /// editable source of truth from first run onward) and returns the
    /// defaults.
    pub fn load_or_init(path: &Path) -> Result<Self> {
        if path.exists() {
            let raw = fs::read_to_string(path).map_err(|e| AlynxisError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            let cfg: Config = toml::from_str(&raw).map_err(|e| AlynxisError::ConfigParse {
                path: path.to_path_buf(),
                source: e,
            })?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.save(path)?;
            Ok(cfg)
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AlynxisError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let raw = toml::to_string_pretty(self)
            .map_err(|e| AlynxisError::ConfigSerialize { source: e })?;
        fs::write(path, raw).map_err(|e| AlynxisError::Io {
            path: path.to_path_buf(),
            source: e,
        })
    }

    /// Ensures `data_dir` and its standard subdirectories exist on disk.
    pub fn ensure_data_dirs(&self) -> Result<()> {
        for sub in ["", "logs", "state"] {
            let p = self.data_dir.join(sub);
            fs::create_dir_all(&p).map_err(|e| AlynxisError::Io { path: p, source: e })?;
        }
        Ok(())
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.data_dir.join("state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let cfg = Config::default();
        assert!(
            cfg.require_zone_b_review,
            "Section 9a requires this to default true"
        );
        assert_eq!(cfg.admin_inactivity_timeout_secs, 1800);
    }

    #[test]
    fn load_or_init_creates_file_then_reloads_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("alynxis.toml");
        assert!(!path.exists());

        let created = Config::load_or_init(&path).unwrap();
        assert!(path.exists());

        let reloaded = Config::load_or_init(&path).unwrap();
        assert_eq!(
            created.require_zone_b_review,
            reloaded.require_zone_b_review
        );
        assert_eq!(
            created.admin_inactivity_timeout_secs,
            reloaded.admin_inactivity_timeout_secs
        );
    }

    #[test]
    fn partial_toml_fills_in_missing_fields_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("alynxis.toml");
        fs::write(&path, "require_zone_b_review = false\n").unwrap();

        let cfg = Config::load_or_init(&path).unwrap();
        assert!(!cfg.require_zone_b_review);
        // Everything else should still fall back to defaults via serde(default).
        assert_eq!(cfg.admin_inactivity_timeout_secs, 1800);
    }

    #[test]
    fn ensure_data_dirs_creates_expected_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            data_dir: dir.path().join("nested").join("alynxis_data"),
            ..Config::default()
        };
        cfg.ensure_data_dirs().unwrap();
        assert!(cfg.data_dir.exists());
        assert!(cfg.logs_dir().exists());
        assert!(cfg.state_dir().exists());
    }
}
