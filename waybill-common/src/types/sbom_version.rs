//! `SbomVersion` — the CDX 1.6 `metadata.version` slot, enforced at
//! construction to match `{"type": "integer", "minimum": 1}` per the
//! CycloneDX 1.6 schema.
//!
//! Emission targets (per contracts/sbom-emission-contract.md):
//! - CDX 1.6: `metadata.version` as a native JSON integer.
//! - SPDX 2.3 / SPDX 3: rendered as a decimal string via `Display`
//!   for inclusion in a `waybill:sbom-version=<N>` annotation
//!   key=value pair.
//!
//! Rejects (per spec FR-014): non-integers (`2.0`, `v2`, `latest`,
//! empty), values `< 1`, embedded whitespace or control chars.

use core::fmt;
use core::num::NonZeroU32;
use core::str::FromStr;
use serde::Serialize;
use thiserror::Error;

/// Positive-integer SBOM document version. See module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct SbomVersion(NonZeroU32);

impl SbomVersion {
    /// The default when `--sbom-version` is unset. Equals `1` —
    /// matches the pre-feature hardcoded CDX `metadata.version` value.
    pub const DEFAULT: SbomVersion = SbomVersion(NonZeroU32::MIN);

    /// Underlying u32.
    pub fn as_u32(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for SbomVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.get().fmt(f)
    }
}

impl FromStr for SbomVersion {
    type Err = SbomVersionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let n: u32 = raw.parse().map_err(|_| SbomVersionError::NotInteger)?;
        NonZeroU32::new(n)
            .map(SbomVersion)
            .ok_or(SbomVersionError::LessThanOne)
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SbomVersionError {
    #[error(
        "--sbom-version must be a positive integer (matches CDX 1.6 metadata.version schema)"
    )]
    NotInteger,
    #[error("--sbom-version must be >= 1")]
    LessThanOne,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn default_is_one() {
        assert_eq!(SbomVersion::DEFAULT.as_u32(), 1);
    }

    #[test]
    fn parses_valid_positive_integers() {
        assert_eq!("1".parse::<SbomVersion>().unwrap().as_u32(), 1);
        assert_eq!("42".parse::<SbomVersion>().unwrap().as_u32(), 42);
        assert_eq!(
            "4294967295".parse::<SbomVersion>().unwrap().as_u32(),
            u32::MAX
        );
    }

    #[test]
    fn rejects_zero_and_negative() {
        assert_eq!("0".parse::<SbomVersion>(), Err(SbomVersionError::LessThanOne));
        // Negative parses as NotInteger because u32::from_str rejects leading `-`.
        assert_eq!(
            "-1".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
    }

    #[test]
    fn rejects_non_integer_syntax() {
        assert_eq!(
            "2.0".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
        assert_eq!("v2".parse::<SbomVersion>(), Err(SbomVersionError::NotInteger));
        assert_eq!(
            "latest".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
        assert_eq!("".parse::<SbomVersion>(), Err(SbomVersionError::NotInteger));
    }

    #[test]
    fn rejects_embedded_whitespace_and_control_chars() {
        assert_eq!(" 2".parse::<SbomVersion>(), Err(SbomVersionError::NotInteger));
        assert_eq!("2 ".parse::<SbomVersion>(), Err(SbomVersionError::NotInteger));
        assert_eq!(
            "2\n".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
        assert_eq!(
            "2\t".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
        assert_eq!(
            "2\0".parse::<SbomVersion>(),
            Err(SbomVersionError::NotInteger)
        );
    }

    #[test]
    fn serializes_as_bare_integer() {
        let v: SbomVersion = "7".parse().unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "7");
    }

    #[test]
    fn displays_as_decimal_string() {
        let v: SbomVersion = "42".parse().unwrap();
        assert_eq!(v.to_string(), "42");
    }
}
