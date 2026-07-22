//! Belief confidence/precision (Section 2b) — minimal Part 2 form.
//!
//! Every node and edge in the WorldModel carries one of these. Section 2b
//! describes a much richer mechanism than what's implemented here:
//! per-(source, domain) reliability tracking, corroboration across
//! independent sources, and an active re-derivation pass when a trusted
//! source issues a correction. None of that exists yet — it needs agent
//! and source modeling that doesn't arrive until the Theory Engine (Parts
//! 17–18) and the deliberate-reasoning machinery of System 2 (Part 10).
//!
//! What Part 2 lays down instead: the confidence *value* itself, plus the
//! basic update primitives (self-verification, corroboration,
//! disconfirmation, recency-weighting) that Section 2b's factor list
//! names as the inputs to that value. Later parts build the fuller
//! machinery on top of this rather than replacing it.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// How strongly a single update nudges `precision`. Self-verification
/// (Alynxis directly tested/verified the belief itself) carries the most
/// weight per Section 2b: "Self-verified evidence should carry more weight
/// than assertion from any source, including Alynxis's own past unverified
/// output."
const SELF_VERIFICATION_STEP: f64 = 0.15;
/// Independent corroboration from another source — weaker than direct
/// self-verification, per Section 2b's explicit ordering.
const CORROBORATION_STEP: f64 = 0.08;
/// A single disconfirmation's pull on precision. Deliberately smaller in
/// magnitude than a self-verification step in the opposite direction isn't
/// symmetric by design — Section 2b requires resisting a single
/// unsubstantiated correction against a well-verified belief, not
/// collapsing on first contact with pushback.
const DISCONFIRMATION_STEP: f64 = 0.10;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Confidence {
    /// 0.0 (no confidence) .. 1.0 (maximal confidence).
    pub precision: f64,
    pub self_verification_count: u32,
    pub last_updated_unix: u64,
}

impl Confidence {
    /// A freshly-created belief with no verification history yet. Starts
    /// low rather than neutral (0.5) — Section 2b: "A belief Alynxis
    /// genuinely has little verified evidence for should be held with
    /// correspondingly low confidence."
    pub fn new_unverified() -> Self {
        Self {
            precision: 0.2,
            self_verification_count: 0,
            last_updated_unix: now_unix(),
        }
    }

    pub fn record_self_verification(&mut self) {
        self.self_verification_count = self.self_verification_count.saturating_add(1);
        self.precision = (self.precision + SELF_VERIFICATION_STEP).min(1.0);
        self.last_updated_unix = now_unix();
    }

    pub fn record_corroboration(&mut self) {
        self.precision = (self.precision + CORROBORATION_STEP).min(1.0);
        self.last_updated_unix = now_unix();
    }

    /// `strength` in 0.0..=1.0 scales how strong the disconfirming signal
    /// is (e.g. a firm, specific correction vs. a vague one). The actual
    /// pull is also damped by how much verified history this belief
    /// already has, so a single unverified assertion can't override many
    /// rounds of direct self-verification — the resistance-to-
    /// unsubstantiated-correction behavior Section 2b requires.
    pub fn record_disconfirmation(&mut self, strength: f64) {
        let strength = strength.clamp(0.0, 1.0);
        let resistance = 1.0 / (1.0 + self.self_verification_count as f64);
        let pull = DISCONFIRMATION_STEP * strength * resistance.max(0.2);
        self.precision = (self.precision - pull).max(0.0);
        self.last_updated_unix = now_unix();
    }

    /// Recency-weighting (Section 2b: "a recent disconfirmation should
    /// generally count for more than a very old confirmation"). This pulls
    /// precision gently toward the neutral midpoint (0.5) the longer it's
    /// gone without any update — NOT the full Ebbinghaus memory-retention
    /// system (Part 6's job for actual memory nodes), just a basic
    /// time-since-last-update effect on confidence specifically.
    pub fn apply_recency_decay(&mut self, half_life_secs: u64) {
        if half_life_secs == 0 {
            return;
        }
        let now = now_unix();
        let elapsed = now.saturating_sub(self.last_updated_unix) as f64;
        let half_life = half_life_secs as f64;
        let decay_factor = 0.5_f64.powf(elapsed / half_life);
        self.precision = 0.5 + (self.precision - 0.5) * decay_factor;
        // Deliberately not updating last_updated_unix here — decay is a
        // read-time/query-time effect of elapsed time, not itself an
        // "update event" the way verification/corroboration/disconfirmation
        // are. Repeated calls should keep decaying toward 0.5 based on the
        // real last update, not reset the clock on themselves.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_unverified_starts_low_not_neutral() {
        let c = Confidence::new_unverified();
        assert!(c.precision < 0.5);
        assert_eq!(c.self_verification_count, 0);
    }

    #[test]
    fn self_verification_increases_precision_and_count() {
        let mut c = Confidence::new_unverified();
        let before = c.precision;
        c.record_self_verification();
        assert!(c.precision > before);
        assert_eq!(c.self_verification_count, 1);
    }

    #[test]
    fn precision_never_exceeds_one_or_goes_below_zero() {
        let mut c = Confidence::new_unverified();
        for _ in 0..100 {
            c.record_self_verification();
        }
        assert!(c.precision <= 1.0);

        let mut c2 = Confidence::new_unverified();
        for _ in 0..100 {
            c2.record_disconfirmation(1.0);
        }
        assert!(c2.precision >= 0.0);
    }

    #[test]
    fn well_verified_belief_resists_single_unsubstantiated_disconfirmation() {
        let mut c = Confidence::new_unverified();
        for _ in 0..20 {
            c.record_self_verification();
        }
        let strong_precision = c.precision;
        assert!(strong_precision > 0.9);

        c.record_disconfirmation(1.0);
        // A single disconfirmation shouldn't collapse a belief backed by
        // 20 rounds of direct self-verification — Section 2b's explicit
        // requirement.
        assert!(
            c.precision > 0.8,
            "well-verified belief should resist a single unsubstantiated correction, got {}",
            c.precision
        );
    }

    #[test]
    fn corroboration_weaker_than_self_verification() {
        let mut c1 = Confidence::new_unverified();
        c1.record_self_verification();

        let mut c2 = Confidence::new_unverified();
        c2.record_corroboration();

        assert!(c1.precision > c2.precision);
    }

    #[test]
    fn recency_decay_pulls_toward_neutral_midpoint() {
        let mut c = Confidence::new_unverified();
        for _ in 0..20 {
            c.record_self_verification();
        }
        assert!(c.precision > 0.9);
        // Simulate a long time having passed without further reinforcement.
        c.last_updated_unix = c.last_updated_unix.saturating_sub(1_000_000);
        c.apply_recency_decay(3600); // 1-hour half-life, ~1M seconds elapsed
        assert!(
            (c.precision - 0.5).abs() < 0.01,
            "should have decayed nearly all the way to neutral, got {}",
            c.precision
        );
    }

    #[test]
    fn recency_decay_with_zero_half_life_is_a_no_op() {
        let mut c = Confidence::new_unverified();
        let before = c.precision;
        c.apply_recency_decay(0);
        assert_eq!(c.precision, before);
    }
}
