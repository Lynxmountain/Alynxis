//! Alynxis — Part 1: Foundation.
//!
//! See the project brief (`alynxis_project_brief9.md`) for full design
//! rationale. This crate implements only what Section 10 scopes to Part 1:
//! core types, config, a minimal Zone A/B/C safety skeleton, logging, and
//! the admin-credential placeholder. No WorldModel, value system, or
//! memory machinery yet — those arrive starting Part 2.

pub mod config;
pub mod core;
pub mod error;
pub mod ids;
pub mod logging;

pub use config::Config;
pub use error::{AlynxisError, Result};
pub use ids::AlynxisId;
