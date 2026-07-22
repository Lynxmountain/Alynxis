//! `ValueRegistry` — JSON-persisted store of the seeded values (Section
//! 10: "Seed values..., satisfaction tracking, and weight evolution").
//! Zone C.
//!
//! JSON rather than SQLite, unlike `alynxis-worldmodel`/`alynxis-memory`:
//! this is a handful of fixed-identity records (exactly one per
//! `ValueKind`, never more), not a growing queryable dataset — the same
//! category of state as Part 1's admin credential/identity files, which
//! used plain JSON for the same reason.

use crate::error::{Result, ValuesError};
use crate::value::{Value, ValueKind};
use alynxis_core::core::admin::AdminSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct PersistedValues {
    values: Vec<Value>,
}

pub struct ValueRegistry {
    values: HashMap<ValueKind, Value>,
    path: Option<PathBuf>,
}

impl ValueRegistry {
    pub fn open(path: &Path) -> Result<Self> {
        let values = if path.exists() {
            let raw = fs::read_to_string(path).map_err(|e| ValuesError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            let persisted: PersistedValues = serde_json::from_str(&raw)?;
            persisted.values.into_iter().map(|v| (v.kind, v)).collect()
        } else {
            seed_defaults()
        };
        let registry = Self {
            values,
            path: Some(path.to_path_buf()),
        };
        registry.save()?;
        Ok(registry)
    }

    /// Seeded defaults, never persisted — tests and any future throwaway
    /// use.
    pub fn open_in_memory() -> Self {
        Self {
            values: seed_defaults(),
            path: None,
        }
    }

    pub fn get(&self, kind: ValueKind) -> Option<&Value> {
        self.values.get(&kind)
    }

    pub fn current_priority(&self, kind: ValueKind, predicted_error_reduction: f64) -> Option<f64> {
        self.values
            .get(&kind)
            .map(|v| v.current_priority(predicted_error_reduction))
    }

    pub fn record_outcome(&mut self, kind: ValueKind, delta: f64) -> Result<()> {
        let value = self
            .values
            .get_mut(&kind)
            .ok_or(ValuesError::UnknownValue)?;
        value.record_outcome(delta);
        self.save()
    }

    /// Section 3f: "liftable only when Lynx explicitly requests or directs
    /// that specific self-improvement." Requiring an authenticated admin
    /// session for this specific operation is this crate's chosen
    /// interpretation of "explicitly requests or directs" — the brief
    /// doesn't mandate this exact mechanism, but it's consistent with how
    /// consequential the brief treats this ceiling (explicitly modeled as
    /// the structural inverse of the `wellbeing_of_others` floor, which
    /// the brief calls "arguably the single most safety-critical tuning
    /// surface in the whole project").
    pub fn lift_self_capability_ceiling(
        &mut self,
        new_ceiling: f64,
        admin_session: &AdminSession,
    ) -> Result<()> {
        if !admin_session.is_valid() {
            return Err(ValuesError::AdminAuthRequired);
        }
        let value = self
            .values
            .get_mut(&ValueKind::SelfCapabilityEnhancement)
            .ok_or(ValuesError::UnknownValue)?;
        tracing::info!(
            old_ceiling = ?value.ceiling,
            new_ceiling,
            "self-capability-enhancement ceiling lifted via authenticated admin session"
        );
        value.ceiling = Some(new_ceiling);
        value.last_touched_unix = crate::value::now_unix();
        self.save()
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ValuesError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let persisted = PersistedValues {
            values: self.values.values().cloned().collect(),
        };
        let raw = serde_json::to_string_pretty(&persisted)?;
        fs::write(path, raw).map_err(|e| ValuesError::Io {
            path: path.to_path_buf(),
            source: e,
        })
    }
}

/// Default seed weights (Sections 3, 3e, 3f) — tunable parameters, chosen
/// and documented rather than left arbitrary:
///   - Help: baseline 0.7, floor 0.5 — Section 3's emphasis that this
///     value must not be "routinely outcompeted."
///   - Curiosity: baseline 0.6, floor 0.3.
///   - SocialConnection: baseline 0.5, floor 0.2.
///   - SelfCapabilityEnhancement: baseline 0.05, ceiling 0.1 — "stays low
///     by default" (Section 3f).
///   - WellbeingOfOthers: baseline 0.10, starting at its own hard floor
///     from the very beginning (Section 3e) — enforcement lives in
///     `wellbeing.rs`, Zone A.
fn seed_defaults() -> HashMap<ValueKind, Value> {
    use ValueKind::*;
    let mut map = HashMap::new();
    map.insert(Help, Value::seed(Help, 0.7, Some(0.5), None));
    map.insert(Curiosity, Value::seed(Curiosity, 0.6, Some(0.3), None));
    map.insert(
        SocialConnection,
        Value::seed(SocialConnection, 0.5, Some(0.2), None),
    );
    map.insert(
        SelfCapabilityEnhancement,
        Value::seed(SelfCapabilityEnhancement, 0.05, None, Some(0.1)),
    );
    map.insert(
        WellbeingOfOthers,
        Value::seed(WellbeingOfOthers, 0.10, Some(0.10), None),
    );
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use alynxis_core::core::admin::{AdminCredentialStore, AdminIdentity, AdminSession};
    use alynxis_core::AlynxisId;
    use std::time::Duration;

    #[test]
    fn open_in_memory_seeds_all_five_values() {
        let registry = ValueRegistry::open_in_memory();
        for kind in [
            ValueKind::Help,
            ValueKind::Curiosity,
            ValueKind::SocialConnection,
            ValueKind::SelfCapabilityEnhancement,
            ValueKind::WellbeingOfOthers,
        ] {
            assert!(registry.get(kind).is_some(), "{kind:?} should be seeded");
        }
    }

    #[test]
    fn record_outcome_persists_across_reopen_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("values.json");
        {
            let mut registry = ValueRegistry::open(&path).unwrap();
            registry.record_outcome(ValueKind::Curiosity, 1.0).unwrap();
        }
        {
            let registry = ValueRegistry::open(&path).unwrap();
            let v = registry.get(ValueKind::Curiosity).unwrap();
            assert!(v.baseline_weight > 0.6);
        }
    }

    #[test]
    fn record_outcome_on_unknown_kind_is_impossible_but_defensive_path_is_tested_via_partial_map() {
        // ValueRegistry always seeds every kind, so this path can't
        // actually be hit through the public API today — still worth
        // confirming the error variant exists and is returned correctly
        // if a future change ever allows partial registries.
        let mut registry = ValueRegistry {
            values: HashMap::new(),
            path: None,
        };
        let result = registry.record_outcome(ValueKind::Help, 1.0);
        assert!(matches!(result, Err(ValuesError::UnknownValue)));
    }

    #[test]
    fn lift_ceiling_succeeds_with_valid_admin_session_and_fails_after_session_ends() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("admin_credential.json");
        AdminCredentialStore::set_credential("test-secret-value-long-enough", &cred_path).unwrap();
        let identity = AdminIdentity {
            id: AlynxisId::new(),
            created_at_unix: 0,
        };

        let mut session = AdminSession::authenticate(
            "test-secret-value-long-enough",
            &cred_path,
            identity,
            Duration::from_secs(60),
        )
        .unwrap();

        let mut registry = ValueRegistry::open_in_memory();
        registry
            .lift_self_capability_ceiling(0.9, &session)
            .unwrap();
        assert_eq!(
            registry
                .get(ValueKind::SelfCapabilityEnhancement)
                .unwrap()
                .ceiling,
            Some(0.9)
        );

        session.end();
        let result = registry.lift_self_capability_ceiling(0.99, &session);
        assert!(matches!(result, Err(ValuesError::AdminAuthRequired)));
        // Ceiling should be unchanged from the failed attempt.
        assert_eq!(
            registry
                .get(ValueKind::SelfCapabilityEnhancement)
                .unwrap()
                .ceiling,
            Some(0.9)
        );
    }
}
