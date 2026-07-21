//! `CorpusSha` newtype — git-SHA-1 of the fingerprint-corpus
//! sibling-repo commit pinned for this scan.
//!
//! Two display widths matter:
//! - **40-hex** (full): used as the cache directory key
//!   (`~/.cache/mikebom/fingerprints/<full-40-hex>/`) to eliminate
//!   collision risk.
//! - **12-hex** (`git rev-parse --short` default): used as the value
//!   of the `mikebom:fingerprint-corpus-sha` SBOM annotation per
//!   FR-005, for human-readable provenance lookup.
//!
//! The build-time-embedded SHA is resolved via
//! `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` (set by `build.rs` from
//! `<workspace>/tests/fingerprints.rev`).

use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum CorpusShaError {
    #[error("corpus SHA must be 40 lowercase hex characters; got {got_len} chars")]
    WrongLength { got_len: usize },
    #[error("corpus SHA contains non-hex characters: {found:?}")]
    NonHex { found: char },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) struct CorpusSha([u8; 20]);

#[allow(dead_code)]
impl CorpusSha {
    /// Parse a 40-char lowercase-hex string into a CorpusSha.
    pub fn from_hex(s: &str) -> Result<Self, CorpusShaError> {
        if s.len() != 40 {
            return Err(CorpusShaError::WrongLength { got_len: s.len() });
        }
        let mut bytes = [0u8; 20];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let hi = decode_nibble(s.as_bytes()[i * 2])?;
            let lo = decode_nibble(s.as_bytes()[i * 2 + 1])?;
            *byte = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }

    /// 40-char lowercase hex — used as the cache directory key.
    pub fn to_full_hex(self) -> String {
        let mut out = String::with_capacity(40);
        for b in &self.0 {
            out.push(nibble_to_char(b >> 4));
            out.push(nibble_to_char(b & 0x0F));
        }
        out
    }

    /// 12-char lowercase hex — matches `git rev-parse --short` default,
    /// used as the value of the `mikebom:fingerprint-corpus-sha`
    /// SBOM annotation per FR-005.
    pub fn to_short_hex(self) -> String {
        let full = self.to_full_hex();
        full[..12].to_string()
    }

    /// Resolve the build-time-embedded corpus SHA from the
    /// `MIKEBOM_FINGERPRINTS_CORPUS_SHA` env var (set by `build.rs`).
    /// Panics at compile time if the env var isn't set or is malformed,
    /// so production binaries are unforgeable on this axis.
    pub fn build_time_embedded() -> Self {
        const EMBEDDED: &str = env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA");
        Self::from_hex(EMBEDDED).expect(
            "MIKEBOM_FINGERPRINTS_CORPUS_SHA env var (set by build.rs) must be 40-char lowercase hex; build.rs panics on malformed pin so this should be unreachable in production",
        )
    }
}

fn decode_nibble(byte: u8) -> Result<u8, CorpusShaError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(CorpusShaError::NonHex {
            found: byte as char,
        }),
    }
}

fn nibble_to_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => unreachable!("nibble values are 0..=15 by construction"),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const SAMPLE_SHA: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";

    #[test]
    fn from_hex_accepts_lowercase_40_hex() {
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        assert_eq!(sha.to_full_hex(), SAMPLE_SHA);
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        assert!(matches!(
            CorpusSha::from_hex("fff39c6a"),
            Err(CorpusShaError::WrongLength { got_len: 8 })
        ));
        assert!(matches!(
            CorpusSha::from_hex(&"f".repeat(41)),
            Err(CorpusShaError::WrongLength { got_len: 41 })
        ));
    }

    #[test]
    fn from_hex_rejects_non_hex_chars() {
        // 'Z' at position 0 — must be hex.
        let bad: String = format!("Z{}", &SAMPLE_SHA[1..]);
        assert!(matches!(
            CorpusSha::from_hex(&bad),
            Err(CorpusShaError::NonHex { .. })
        ));
        // Uppercase hex — we require lowercase only.
        assert!(matches!(
            CorpusSha::from_hex(&SAMPLE_SHA.to_uppercase()),
            Err(CorpusShaError::NonHex { .. })
        ));
    }

    #[test]
    fn to_short_hex_truncates_to_12_chars() {
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let short = sha.to_short_hex();
        assert_eq!(short.len(), 12);
        assert_eq!(short, "fff39c6ad22c");
        assert!(SAMPLE_SHA.starts_with(&short));
    }

    #[test]
    fn to_full_hex_lowercase_roundtrip() {
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let roundtripped = CorpusSha::from_hex(&sha.to_full_hex()).unwrap();
        assert_eq!(sha, roundtripped);
    }

    #[test]
    fn build_time_embedded_resolves_to_real_sha() {
        // env!() resolves at compile time; if the build.rs change worked,
        // this SHA matches what's in tests/fingerprints.rev.
        let sha = CorpusSha::build_time_embedded();
        let full = sha.to_full_hex();
        assert_eq!(full.len(), 40);
        // All-zeros would indicate a build.rs bug; check at least one
        // non-zero byte.
        assert!(
            full.chars().any(|c| c != '0'),
            "build-time-embedded SHA is all zeros — build.rs likely emitted an empty/missing pin",
        );
    }
}
