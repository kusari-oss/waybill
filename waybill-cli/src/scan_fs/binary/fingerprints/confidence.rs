//! Confidence newtype + fusion bucket enum for corpus v2 matching.
//!
//! Per milestone 110 spec FR-017 and data-model.md:
//! - `Confidence` wraps `f64` in `[0.0, 1.0]`.
//! - `FusedConfidence` is the post-fusion two-bucket emission decision:
//!   `High` (>=0.85), `Medium` (>=0.70). Below 0.70 → `None` (suppressed).
//!
//! Constitution principle IV (no `.unwrap()` in production) is preserved at
//! known-baseline construction sites via the `from_pct_in_range_const` const
//! constructor, which uses a `const { assert!(...) }` to enforce the
//! `0..=100` range at compile time.

use serde::{Deserialize, Serialize};

use super::record::CorpusError;

/// A confidence score in `[0.0, 1.0]`. Newtype boundary per constitution
/// principle IV.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub(crate) struct Confidence(f64);

impl TryFrom<f64> for Confidence {
    type Error = CorpusError;
    fn try_from(v: f64) -> Result<Self, Self::Error> {
        if (0.0..=1.0).contains(&v) {
            Ok(Confidence(v))
        } else {
            Err(CorpusError::ConfidenceOutOfRange(v))
        }
    }
}

impl From<Confidence> for f64 {
    fn from(c: Confidence) -> f64 {
        c.0
    }
}

#[allow(dead_code)] // Phase 2: declared; Phase 3 (T017 v1-upgrade) consumes.
impl Confidence {
    /// Const constructor for compile-time-known valid baselines (e.g., the
    /// v1-upgrade 0.70 baseline). `PCT` is in 0..=100 — so
    /// `from_pct_in_range_const::<70>()` constructs `Confidence(0.70)`.
    ///
    /// Out-of-range `PCT` is a compile-time error via `const { assert!(...) }`
    /// inside the function body, so this constructor cannot panic at runtime
    /// and satisfies constitution principle IV's no-`.unwrap()`-in-production
    /// rule for fixed-baseline call sites.
    pub(crate) const fn from_pct_in_range_const<const PCT: u8>() -> Self {
        const { assert!(PCT <= 100, "Confidence percentage must be 0..=100"); }
        Self(PCT as f64 / 100.0)
    }

    pub(crate) const fn into_inner(self) -> f64 {
        self.0
    }
}

/// Post-fusion bucket — what the matcher actually emits.
///
/// No `Low` variant. Per the 2026-06-03 /speckit-clarify Q1 clarification +
/// spec FR-017, matches whose fused confidence falls below the `Medium`
/// threshold (0.70) are suppressed entirely — the matcher returns `None`
/// rather than emitting a low-confidence component. This encodes the
/// suppression-below-floor rule at the type level so the matcher cannot
/// accidentally emit a "low" component.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)] // Phase 2: types declared; Phase 3+ wire emission.
pub(crate) enum FusedConfidence {
    /// confidence >= 0.85
    High,
    /// 0.70 <= confidence < 0.85
    Medium,
}

#[allow(dead_code)] // Phase 2: declared; Phase 4 (matcher) consumes.
impl FusedConfidence {
    /// Bucket a fused-confidence value. Returns `None` for values below the
    /// `Medium` floor (< 0.70) per the spec's suppression-below-floor rule.
    pub(crate) fn from_fused(c: Confidence) -> Option<Self> {
        let v = c.into_inner();
        if v >= 0.85 {
            Some(Self::High)
        } else if v >= 0.70 {
            Some(Self::Medium)
        } else {
            None
        }
    }

    /// SBOM-annotation-friendly bucket name. Stable across waybill versions
    /// per FR-017 ("version-stable fusion rule"); consumers can pattern-match
    /// on the string.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn confidence_accepts_in_range_values() {
        assert!(Confidence::try_from(0.0).is_ok());
        assert!(Confidence::try_from(0.5).is_ok());
        assert!(Confidence::try_from(1.0).is_ok());
    }

    #[test]
    fn confidence_rejects_out_of_range() {
        assert!(Confidence::try_from(-0.1).is_err());
        assert!(Confidence::try_from(1.1).is_err());
        assert!(Confidence::try_from(f64::NAN).is_err());
    }

    #[test]
    fn const_constructor_produces_expected_value() {
        let c = Confidence::from_pct_in_range_const::<70>();
        assert!((c.into_inner() - 0.70).abs() < 1e-9);
        let c = Confidence::from_pct_in_range_const::<95>();
        assert!((c.into_inner() - 0.95).abs() < 1e-9);
        let c = Confidence::from_pct_in_range_const::<0>();
        assert_eq!(c.into_inner(), 0.0);
        let c = Confidence::from_pct_in_range_const::<100>();
        assert_eq!(c.into_inner(), 1.0);
    }

    #[test]
    fn fused_confidence_buckets_correctly() {
        let high = Confidence::try_from(0.95).unwrap();
        assert_eq!(FusedConfidence::from_fused(high), Some(FusedConfidence::High));
        let exact_high = Confidence::try_from(0.85).unwrap();
        assert_eq!(FusedConfidence::from_fused(exact_high), Some(FusedConfidence::High));
        let medium = Confidence::try_from(0.75).unwrap();
        assert_eq!(FusedConfidence::from_fused(medium), Some(FusedConfidence::Medium));
        let exact_medium = Confidence::try_from(0.70).unwrap();
        assert_eq!(FusedConfidence::from_fused(exact_medium), Some(FusedConfidence::Medium));
        let below_floor = Confidence::try_from(0.69).unwrap();
        assert_eq!(FusedConfidence::from_fused(below_floor), None);
        let weak = Confidence::try_from(0.40).unwrap();
        assert_eq!(FusedConfidence::from_fused(weak), None);
    }

    #[test]
    fn fused_confidence_as_str_is_stable() {
        // FR-017: bucket name strings are version-stable.
        assert_eq!(FusedConfidence::High.as_str(), "high");
        assert_eq!(FusedConfidence::Medium.as_str(), "medium");
    }

    #[test]
    fn confidence_round_trips_via_serde() {
        let c = Confidence::try_from(0.85).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "0.85");
        let back: Confidence = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn confidence_rejects_out_of_range_at_deserialize() {
        let result: Result<Confidence, _> = serde_json::from_str("1.5");
        assert!(result.is_err());
    }
}
