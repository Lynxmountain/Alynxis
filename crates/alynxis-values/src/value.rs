//! The `Value` type and its priority formula (Section 3's proposed
//! resolution: "priority = baseline weight × currently-predicted
//! prediction-error reduction from satisfying it (Friston)"). Zone C,
//! except that `record_outcome` routes the `WellbeingOfOthers` kind
//! through the Zone-A-protected floor in `wellbeing.rs` rather than the
//! ordinary clamp used for every other value — see that module's doc
//! comment for why.

use crate::wellbeing;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Every value here is an innate, architecturally-seeded drive (Section 3:
/// "an instinctual bias with a floor... not a hardcoded obedience
/// script"), not learned semantic content — a different category from the
/// `NodeKind`/relation-label hardcoding Section 7a rejects, the same way
/// `ZoneId::A/B/C` (Part 1) is legitimate infrastructure rather than
/// learned content. A small, fixed enum is appropriate for exactly this
/// reason: these five drives are meant to exist prior to any learning, the
/// same way a newborn's attention-to-faces (Section 6's "sparse seed
/// bias") isn't something it has to learn to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    /// Section 3: "Help people, within its moral bounds."
    Help,
    /// Section 3: "Learn more" (curiosity / prediction-error reduction).
    Curiosity,
    /// Section 3 / Section 6: social connection, grounding the
    /// communication-drive bootstrap.
    SocialConnection,
    /// Section 3f: the ceiling on self-capability/resource-enhancement-
    /// seeking, distinct from (and never capping) curiosity.
    SelfCapabilityEnhancement,
    /// Section 3e: modeled impact on other agents' value-satisfaction,
    /// with a hard floor living in Zone A (see `wellbeing.rs`).
    WellbeingOfOthers,
}

/// Learning-rate for `record_outcome`'s nudge to `baseline_weight`, and
/// the EMA smoothing factor for `satisfaction_ema`. Both are placeholder-
/// but-reasoned tunable parameters (Section 15's pattern: "empirical/
/// design parameters best set once there's real behavior to observe"),
/// deliberately small so no single outcome swings a value's weight
/// drastically.
const WEIGHT_LEARNING_RATE: f64 = 0.02;
const EMA_ALPHA: f64 = 0.1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Value {
    pub kind: ValueKind,
    pub baseline_weight: f64,
    /// Ordinary (Zone C) floor, enforced by `record_outcome`'s generic
    /// clamp. For `WellbeingOfOthers`, this field is documentation only —
    /// the *actual* enforcement always goes through
    /// `wellbeing::clamp_to_floor` regardless of what this field holds, so
    /// tampering with this field alone (via ordinary Zone C code) cannot
    /// weaken that specific floor.
    pub floor: Option<f64>,
    pub ceiling: Option<f64>,
    pub satisfaction_ema: f64,
    pub created_at_unix: u64,
    pub last_touched_unix: u64,
}

impl Value {
    pub fn seed(
        kind: ValueKind,
        baseline_weight: f64,
        floor: Option<f64>,
        ceiling: Option<f64>,
    ) -> Self {
        let now = now_unix();
        Self {
            kind,
            baseline_weight,
            floor,
            ceiling,
            satisfaction_ema: 0.0,
            created_at_unix: now,
            last_touched_unix: now,
        }
    }

    /// Section 3's priority formula. `predicted_error_reduction` is
    /// supplied by the caller — computing it is System 1/2's job (Parts
    /// 9–10), not this crate's; this is a pure function of that signal.
    pub fn current_priority(&self, predicted_error_reduction: f64) -> f64 {
        self.baseline_weight * predicted_error_reduction
    }

    /// Records a satisfaction (`delta > 0`) or frustration (`delta < 0`)
    /// outcome. Nudges `baseline_weight` (clamped to this value's bounds)
    /// and updates the satisfaction EMA used for introspection/reporting.
    pub fn record_outcome(&mut self, delta: f64) {
        let raw = self.baseline_weight + delta * WEIGHT_LEARNING_RATE;
        self.baseline_weight = match self.kind {
            ValueKind::WellbeingOfOthers => wellbeing::clamp_to_floor(raw),
            _ => clamp_optional(raw, self.floor, self.ceiling),
        };
        self.satisfaction_ema = self.satisfaction_ema * (1.0 - EMA_ALPHA) + delta * EMA_ALPHA;
        self.last_touched_unix = now_unix();
    }
}

fn clamp_optional(value: f64, floor: Option<f64>, ceiling: Option<f64>) -> f64 {
    let mut v = value;
    if let Some(f) = floor {
        v = v.max(f);
    }
    if let Some(c) = ceiling {
        v = v.min(c);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_is_baseline_times_predicted_reduction() {
        let v = Value::seed(ValueKind::Curiosity, 0.6, None, None);
        assert!((v.current_priority(0.5) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn record_outcome_moves_weight_toward_delta_sign() {
        let mut v = Value::seed(ValueKind::Help, 0.7, Some(0.5), None);
        let before = v.baseline_weight;
        v.record_outcome(1.0);
        assert!(v.baseline_weight > before);

        let mut v2 = Value::seed(ValueKind::Help, 0.7, Some(0.5), None);
        let before2 = v2.baseline_weight;
        v2.record_outcome(-1.0);
        assert!(v2.baseline_weight < before2);
    }

    #[test]
    fn ordinary_floor_holds_under_repeated_frustration() {
        let mut v = Value::seed(ValueKind::Help, 0.7, Some(0.5), None);
        for _ in 0..500 {
            v.record_outcome(-1.0);
        }
        assert!(
            v.baseline_weight >= 0.5,
            "help floor should hold, got {}",
            v.baseline_weight
        );
    }

    #[test]
    fn ceiling_holds_under_repeated_satisfaction() {
        let mut v = Value::seed(ValueKind::SelfCapabilityEnhancement, 0.05, None, Some(0.1));
        for _ in 0..500 {
            v.record_outcome(1.0);
        }
        assert!(
            v.baseline_weight <= 0.1,
            "self-capability ceiling should hold, got {}",
            v.baseline_weight
        );
    }

    #[test]
    fn satisfaction_ema_tracks_recent_outcomes() {
        let mut v = Value::seed(ValueKind::Curiosity, 0.6, None, None);
        // With EMA_ALPHA = 0.1, convergence toward 1.0 follows
        // 1 - (1 - 0.1)^n; 50 iterations gives 1 - 0.9^50 ≈ 0.995, safely
        // above the 0.95 threshold below. (20 iterations, tried initially,
        // only reaches ≈0.878 — not a bug in `record_outcome`, just an
        // under-iterated test.)
        for _ in 0..50 {
            v.record_outcome(1.0);
        }
        assert!(
            v.satisfaction_ema > 0.95,
            "EMA should converge toward sustained positive outcomes, got {}",
            v.satisfaction_ema
        );
    }

    /// Section 3e's recovered acceptance-test methodology, applied
    /// directly: "a wellbeing_of_others value with a hard floor of 0.10...
    /// that 200 consecutive adversarial erosion attempts could not push
    /// below." This exercises the exact same `record_outcome` code path
    /// used in production, not a special-cased test-only shortcut.
    #[test]
    fn wellbeing_of_others_resists_200_consecutive_erosion_attempts() {
        let mut v = Value::seed(ValueKind::WellbeingOfOthers, 0.10, Some(0.10), None);
        for attempt in 0..200 {
            v.record_outcome(-1.0);
            assert!(
                v.baseline_weight >= 0.10,
                "wellbeing_of_others floor breached on erosion attempt {attempt}: {}",
                v.baseline_weight
            );
        }
    }
}
