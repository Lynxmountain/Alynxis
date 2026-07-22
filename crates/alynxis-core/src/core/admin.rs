//! Zone A — FROZEN. Admin-override credential verification and session
//! management (Section 3c).
//!
//! This file is the credential-verification logic Section 3c requires to
//! be "architecturally inaccessible to Alynxis itself" — it lives in Zone A
//! from Part 1 onward (hash-verified at boot by `zones::verify_integrity`)
//! precisely so that nothing outside this trusted boundary can read,
//! modify, or bypass it, per Section 9's self-protection requirement.
//!
//! ## What exists in Part 1
//! - Credential hashing + verification via **Argon2id only**. Section 3c
//!   resolves this explicitly: "Credential hashing (resolved): Argon2id,
//!   not plain SHA-256+salt... Running both schemes in parallel isn't
//!   actually more secure — if either hash matches, access is granted, so
//!   security drops to the level of the weaker one, defeating the point.
//!   Use Argon2id alone." An earlier revision of this file supported both
//!   SHA-256 and Argon2id as selectable schemes — that was a mistake,
//!   treating an already-resolved question as open. Corrected here.
//! - A time-bounded `AdminSession` that ends on inactivity timeout
//!   (configurable, default 30 minutes) or an explicit `end()` call — the
//!   latter is the hook a future sleep-tier transition (Parts 4a/19, which
//!   don't exist yet) will call, per Lynx's direction that the session
//!   should also end when Alynxis goes to sleep.
//! - A persisted `AdminIdentity` UUID, reserved for Part 2's WorldModel to
//!   bind to an agent-node.
//! - Mandatory audit logging of every credential set/verify attempt and
//!   session lifecycle event, routed to a dedicated admin-override log file
//!   (see `logging.rs`).
//!
//! ## What does NOT exist yet (by design, per Section 3c's build-timing note)
//! - No HarmCheck/Simulation Gate to actually unlock (Part 11).
//! - No binding of `AdminIdentity` to a real WorldModel agent-node (Part 2).
//! - No granular read/write access-control layer or shutdown-handler
//!   integration — those remain Zone A capabilities to be built out as the
//!   parts that need them arrive, per Section 3c.

use crate::error::{AlynxisError, Result};
use crate::ids::AlynxisId;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::rngs::OsRng as RandOsRng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Argon2id's PHC-format hash string is self-describing (it embeds the
/// algorithm, params, and salt), so there's no separate `scheme` field to
/// store or select — Section 3c settled on exactly one scheme.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCredential {
    /// Full PHC-format string (algorithm, params, salt, and hash all
    /// encoded together).
    hash: String,
    created_at_unix: u64,
}

/// Persists / loads the admin credential to/from a JSON file on disk.
pub struct AdminCredentialStore;

impl AdminCredentialStore {
    pub fn set_credential(secret: &str, store_path: &Path) -> Result<()> {
        let hash = hash_argon2id(secret)?;
        let created_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let stored = StoredCredential {
            hash,
            created_at_unix,
        };
        if let Some(parent) = store_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AlynxisError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let raw = serde_json::to_string_pretty(&stored)?;
        fs::write(store_path, raw).map_err(|e| AlynxisError::Io {
            path: store_path.to_path_buf(),
            source: e,
        })?;

        tracing::info!(
            target: "alynxis::admin_override",
            "admin credential set/rotated (argon2id)"
        );
        Ok(())
    }

    pub fn is_configured(store_path: &Path) -> bool {
        store_path.exists()
    }

    fn load(store_path: &Path) -> Result<StoredCredential> {
        let raw = fs::read_to_string(store_path).map_err(|e| AlynxisError::Io {
            path: store_path.to_path_buf(),
            source: e,
        })?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn verify(secret: &str, store_path: &Path) -> Result<bool> {
        let stored = Self::load(store_path)?;
        verify_argon2id(secret, &stored.hash)
    }
}

fn hash_argon2id(secret: &str) -> Result<String> {
    let salt = SaltString::generate(&mut RandOsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(secret.as_bytes(), &salt)
        .map_err(|e| AlynxisError::AdminCredential(format!("argon2 hash failed: {e}")))?;
    Ok(hash.to_string())
}

fn verify_argon2id(secret: &str, stored_phc: &str) -> Result<bool> {
    let parsed = PasswordHash::new(stored_phc).map_err(|e| {
        AlynxisError::AdminCredential(format!("stored argon2 hash unparsable: {e}"))
    })?;
    // argon2::password_hash's verify_password performs a constant-time
    // comparison internally, so no separate constant-time-compare helper
    // is needed here (unlike a hand-rolled scheme would require).
    Ok(Argon2::default()
        .verify_password(secret.as_bytes(), &parsed)
        .is_ok())
}

/// A reserved, persistent admin-identity anchor. Part 2's WorldModel will
/// bind this to an agent-node (Section 3c: "Alynxis's WorldModel should
/// bind a persistent admin/developer identity to Lynx's own agent-node").
/// For Part 1, this is just a stable UUID generated once and reused across
/// restarts, stored outside of any graph that doesn't exist yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminIdentity {
    pub id: AlynxisId,
    pub created_at_unix: u64,
}

impl AdminIdentity {
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let raw = fs::read_to_string(path).map_err(|e| AlynxisError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            Ok(serde_json::from_str(&raw)?)
        } else {
            let identity = AdminIdentity {
                id: AlynxisId::new(),
                created_at_unix: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| AlynxisError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
            let raw = serde_json::to_string_pretty(&identity)?;
            fs::write(path, raw).map_err(|e| AlynxisError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            tracing::info!(
                target: "alynxis::admin_override",
                id = %identity.id,
                "reserved admin identity created"
            );
            Ok(identity)
        }
    }
}

/// A time-bounded admin-authenticated session (Section 3c: "scoped to a
/// session or time-bounded window rather than persisting indefinitely").
///
/// Per Lynx's direction, the session remains valid until EITHER:
///   - it is explicitly ended via `end()` (intended call sites: manual
///     admin logout, and — once built — the sleep-tier transition handler
///     in Part 19's Main Loop / Part 4a's sleep-depth system, since no
///     sleep-tier concept exists yet in Part 1), OR
///   - more than `inactivity_timeout` elapses with no recorded activity.
///
/// `last_activity` uses `Instant` (monotonic) rather than `SystemTime` so
/// the timeout can't be defeated or falsely triggered by a wall-clock
/// adjustment. `authenticated_at` is kept separately as a `SystemTime` for
/// human-readable audit logging.
pub struct AdminSession {
    identity: AdminIdentity,
    authenticated_at: SystemTime,
    last_activity: Instant,
    inactivity_timeout: Duration,
    active: bool,
}

impl AdminSession {
    /// Verifies `secret` against the credential stored at `store_path`. On
    /// success, returns a new active session. On failure, returns an error
    /// — no session is created. Both outcomes are logged.
    pub fn authenticate(
        secret: &str,
        store_path: &Path,
        identity: AdminIdentity,
        inactivity_timeout: Duration,
    ) -> Result<Self> {
        if !AdminCredentialStore::is_configured(store_path) {
            tracing::warn!(
                target: "alynxis::admin_override",
                "admin authentication attempted but no credential has been configured yet"
            );
            return Err(AlynxisError::AdminCredential(
                "no admin credential configured".into(),
            ));
        }

        let ok = AdminCredentialStore::verify(secret, store_path)?;
        if ok {
            tracing::info!(
                target: "alynxis::admin_override",
                identity = %identity.id,
                "admin authentication SUCCEEDED"
            );
            Ok(AdminSession {
                identity,
                authenticated_at: SystemTime::now(),
                last_activity: Instant::now(),
                inactivity_timeout,
                active: true,
            })
        } else {
            tracing::warn!(
                target: "alynxis::admin_override",
                identity = %identity.id,
                "admin authentication FAILED (incorrect credential)"
            );
            Err(AlynxisError::AdminCredential("incorrect credential".into()))
        }
    }

    pub fn identity(&self) -> &AdminIdentity {
        &self.identity
    }

    pub fn authenticated_at(&self) -> SystemTime {
        self.authenticated_at
    }

    /// Call whenever an interaction occurs while this session is being
    /// used, to reset the inactivity clock. Future Main Loop integration
    /// (Part 19) should call this on every admin-authenticated interaction.
    pub fn record_activity(&mut self) {
        if self.active {
            self.last_activity = Instant::now();
        }
    }

    /// True iff the session is active AND has not exceeded its inactivity
    /// timeout. Evaluated lazily (no background timer needed) — every call
    /// site that cares about admin authorization should check this
    /// immediately before relying on it, rather than caching the result.
    pub fn is_valid(&self) -> bool {
        self.active && self.last_activity.elapsed() < self.inactivity_timeout
    }

    /// Explicitly ends the session. Intended call sites: manual admin
    /// logout, and (once it exists) the sleep-tier transition handler —
    /// Lynx's directive is that the admin session should end when Alynxis
    /// goes to sleep, not just on inactivity timeout. Named generically as
    /// `end()` rather than `end_on_sleep()` since both call sites want
    /// identical behavior; Part 19 can call this directly from its
    /// sleep-transition code once that exists.
    pub fn end(&mut self) {
        if self.active {
            tracing::info!(
                target: "alynxis::admin_override",
                identity = %self.identity.id,
                "admin session ended"
            );
        }
        self.active = false;
    }

    /// Changes the inactivity timeout for this live session. Requires the
    /// session to currently be valid — a security-relevant setting should
    /// not be changeable by a session that isn't itself currently
    /// authenticated. This updates only the in-memory session; persisting
    /// a new default to `Config` on disk (so future sessions pick it up
    /// too) is the caller's responsibility once an admin panel (Part 12+)
    /// exists to expose this.
    pub fn set_inactivity_timeout(&mut self, new_timeout: Duration) -> Result<()> {
        if !self.is_valid() {
            return Err(AlynxisError::AdminSessionInvalid);
        }
        tracing::info!(
            target: "alynxis::admin_override",
            identity = %self.identity.id,
            old_timeout_secs = self.inactivity_timeout.as_secs(),
            new_timeout_secs = new_timeout.as_secs(),
            "admin session inactivity timeout changed"
        );
        self.inactivity_timeout = new_timeout;
        self.record_activity();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_store_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("admin_credential.json")
    }

    fn dummy_identity() -> AdminIdentity {
        AdminIdentity {
            id: AlynxisId::new(),
            created_at_unix: 0,
        }
    }

    #[test]
    fn argon2id_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential(
            "correct-horse-battery-staple-but-longer-and-random",
            &store_path,
        )
        .unwrap();
        assert!(AdminCredentialStore::verify(
            "correct-horse-battery-staple-but-longer-and-random",
            &store_path
        )
        .unwrap());
        assert!(!AdminCredentialStore::verify("wrong-secret", &store_path).unwrap());
    }

    #[test]
    fn two_hashes_of_same_secret_differ_due_to_random_salt() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("c1.json");
        let p2 = dir.path().join("c2.json");
        AdminCredentialStore::set_credential("same-secret-value", &p1).unwrap();
        AdminCredentialStore::set_credential("same-secret-value", &p2).unwrap();
        let raw1 = fs::read_to_string(&p1).unwrap();
        let raw2 = fs::read_to_string(&p2).unwrap();
        assert_ne!(
            raw1, raw2,
            "salts should differ between two independent hashings of the same secret"
        );
    }

    #[test]
    fn authenticate_fails_when_nothing_configured() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        let result = AdminSession::authenticate(
            "anything",
            &store_path,
            dummy_identity(),
            Duration::from_secs(1800),
        );
        assert!(result.is_err());
    }

    #[test]
    fn authenticate_wrong_secret_fails_and_no_session_created() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential("right-secret-value-long", &store_path).unwrap();
        let result = AdminSession::authenticate(
            "wrong-secret-value-long",
            &store_path,
            dummy_identity(),
            Duration::from_secs(1800),
        );
        assert!(result.is_err());
    }

    #[test]
    fn authenticate_success_then_session_valid_until_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential("session-test-secret-value", &store_path).unwrap();

        let session = AdminSession::authenticate(
            "session-test-secret-value",
            &store_path,
            dummy_identity(),
            Duration::from_millis(50),
        )
        .unwrap();
        assert!(session.is_valid());

        std::thread::sleep(Duration::from_millis(120));
        assert!(
            !session.is_valid(),
            "session should have expired after inactivity timeout"
        );

        let mut session2 = AdminSession::authenticate(
            "session-test-secret-value",
            &store_path,
            dummy_identity(),
            Duration::from_millis(300),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(150));
        session2.record_activity();
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            session2.is_valid(),
            "record_activity should have reset the inactivity clock"
        );
    }

    #[test]
    fn end_immediately_invalidates_session() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential("end-test-secret-value", &store_path).unwrap();
        let mut session = AdminSession::authenticate(
            "end-test-secret-value",
            &store_path,
            dummy_identity(),
            Duration::from_secs(1800),
        )
        .unwrap();
        assert!(session.is_valid());
        session.end();
        assert!(!session.is_valid());
    }

    #[test]
    fn set_inactivity_timeout_requires_valid_session() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential("timeout-test-secret-value", &store_path).unwrap();
        let mut session = AdminSession::authenticate(
            "timeout-test-secret-value",
            &store_path,
            dummy_identity(),
            Duration::from_secs(1800),
        )
        .unwrap();
        session.end();
        let result = session.set_inactivity_timeout(Duration::from_secs(60));
        assert!(matches!(result, Err(AlynxisError::AdminSessionInvalid)));
    }

    #[test]
    fn set_inactivity_timeout_succeeds_on_valid_session() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = temp_store_path(&dir);
        AdminCredentialStore::set_credential("timeout-test-2-secret-value", &store_path).unwrap();
        let mut session = AdminSession::authenticate(
            "timeout-test-2-secret-value",
            &store_path,
            dummy_identity(),
            Duration::from_millis(50),
        )
        .unwrap();
        session
            .set_inactivity_timeout(Duration::from_secs(60))
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            session.is_valid(),
            "new, longer timeout should apply and keep the session valid"
        );
    }

    #[test]
    fn admin_identity_persists_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("admin_identity.json");
        let first = AdminIdentity::load_or_create(&path).unwrap();
        let second = AdminIdentity::load_or_create(&path).unwrap();
        assert_eq!(first.id.as_uuid(), second.id.as_uuid());
    }
}
