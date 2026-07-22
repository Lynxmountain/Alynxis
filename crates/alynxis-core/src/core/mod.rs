//! Zone A/B/C safety architecture (Section 9) and the admin-override
//! mechanism (Section 3c).
//!
//! Zone A (frozen, self-protecting) currently consists of exactly the three
//! files in this `core/` module: `zones.rs`, `harm_check.rs`, `admin.rs`.
//! Nothing outside this module is Zone A. As later parts are built,
//! `zones::ZONE_REGISTRY` will be extended to classify their files as Zone
//! B or Zone C — never silently defaulting new safety-relevant code into
//! Zone A without an explicit decision to do so, and never removing
//! anything from Zone A without Lynx's explicit direction.

pub mod admin;
pub mod harm_check;
pub mod zones;
