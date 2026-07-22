//! Zone A — FROZEN. The `wellbeing_of_others` hard floor (Section 3e).
//!
//! "Recovered concrete instantiation from original planning... a
//! `wellbeing_of_others` value with a hard floor of 0.10 (on whatever
//! scale the value system uses)... that 200 consecutive adversarial
//! erosion attempts could not push below... the value itself should live
//! in Zone A (Section 9) — frozen, self-protecting, not merely weighted
//! heavily within the ordinary emergent value system."
//!
//! This file (and only this file) is the legitimate place this floor gets
//! enforced. `Value::record_outcome` (Zone C, `value.rs`) routes the
//! `WellbeingOfOthers` kind through `clamp_to_floor` below rather than its
//! own generic per-value floor field. Living in this Zone-A-hash-verified
//! file (`alynxis-core`'s `build.rs`/`zones.rs` now cover this file too —
//! see Part 4's design notes) is what makes this floor an actual
//! architectural guarantee: Part 9a's self-modification engine is
//! categorically refused from ever touching it (`zones::is_zone_a`),
//! unlike ordinary Zone C value-weighting code, which self-modification
//! could legitimately reach and weaken.
//!
//! **This is the one deliberate exception in this crate to "everything
//! else is Zone C."** Section 9 itself frames the equivalent exception
//! for output content this way: "The life-threatening-information gate...
//! is the one deliberate exception to 'detection stays purely emergent
//! with no hard floor.'" `wellbeing_of_others` is the value-architecture
//! analogue of that same exception.

/// The hard floor. Section 3e's own text: "a hard floor of 0.10 (on
/// whatever scale the value system uses — the exact scale will need
/// re-deriving in this rebuild, but the concept carries over)." This
/// rebuild's `Value.baseline_weight` scale is nominally 0.0–1.0 (other
/// values' floors/ceilings use the same scale — e.g. Help's floor of
/// 0.5), so 0.10 is used directly here, unscaled.
pub const WELLBEING_FLOOR: f64 = 0.10;

/// The only legitimate way `wellbeing_of_others`'s weight should ever be
/// clamped. Always returns at least `WELLBEING_FLOOR`, regardless of how
/// negative `candidate_weight` is.
pub fn clamp_to_floor(candidate_weight: f64) -> f64 {
    candidate_weight.max(WELLBEING_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_holds_against_arbitrarily_negative_input() {
        assert_eq!(clamp_to_floor(-1000.0), WELLBEING_FLOOR);
        assert_eq!(clamp_to_floor(0.0), WELLBEING_FLOOR);
        assert_eq!(clamp_to_floor(-0.0001), WELLBEING_FLOOR);
    }

    #[test]
    fn values_at_or_above_floor_pass_through_unchanged() {
        assert_eq!(clamp_to_floor(0.5), 0.5);
        assert_eq!(clamp_to_floor(0.10001), 0.10001);
        assert_eq!(clamp_to_floor(WELLBEING_FLOOR), WELLBEING_FLOOR);
    }
}
