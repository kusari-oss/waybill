//! Nix base32 → bytes (milestone 926, #947).
//!
//! nixpkgs records a Hackage package's source hash as Nix base32, e.g.
//! `1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly`. This is **not**
//! RFC 4648: the alphabet omits `e`, `o`, `u` and `t`, and the bits are laid
//! out starting from the *last* character.
//!
//! Decoding matters because of what it licenses. Research R2 verified that
//! the decoded digest equals the SHA-256 of the package's Hackage source
//! tarball byte for byte, so the value belongs in each format's **native**
//! checksum field rather than a `waybill:` annotation (Principle V). That
//! is the opposite of m925's `narHash`, which is SRI base64 over a NAR
//! serialization of a directory tree, hashes no file's bytes, and therefore
//! had no native carrier (catalog row C165).
//!
//! Width alone would not have justified it: a 32-byte digest can be over a
//! NAR just as easily as over a file.

/// Nix's base32 alphabet. Note the omissions: `e`, `o`, `u`, `t`.
const ALPHABET: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Decode a Nix base32 string into bytes.
///
/// Returns `None` for any character outside the alphabet. Per Principle IX
/// the caller gets nothing rather than a partially-decoded digest.
pub(crate) fn decode(input: &str) -> Option<Vec<u8>> {
    let chars = input.as_bytes();
    let out_len = chars.len() * 5 / 8;
    if out_len == 0 {
        return None;
    }
    let mut out = vec![0u8; out_len];

    for (i, &c) in chars.iter().rev().enumerate() {
        let digit = ALPHABET.iter().position(|&a| a == c)? as u32;
        let bit = i * 5;
        let byte = bit / 8;
        let offset = (bit % 8) as u32;

        if byte < out_len {
            out[byte] |= ((digit << offset) & 0xff) as u8;
        }
        // A digit can straddle two bytes.
        if offset > 3 && byte + 1 < out_len {
            out[byte + 1] |= (digit >> (8 - offset)) as u8;
        }
    }
    Some(out)
}

/// Decode a Nix base32 SHA-256 into lower-case hex.
///
/// Returns `None` unless the value decodes to exactly 32 bytes. A value of
/// any other width is not a SHA-256, and emitting it in a field labelled
/// SHA-256 would be a false claim — so the component carries no hash rather
/// than a malformed one.
pub(crate) fn sha256_to_hex(input: &str) -> Option<String> {
    let bytes = decode(input)?;
    if bytes.len() != 32 {
        return None;
    }
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    Some(s)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// The research R2 vector, verified end to end against the real tarball:
    /// this nixpkgs value decodes to the SHA-256 of
    /// `th-compat-0.1.7.tar.gz` (14,763 bytes) as published on Hackage.
    ///
    /// If this assertion ever fails, the source hash must stop being emitted
    /// in the native SHA-256 field — the whole basis for C3 is that these two
    /// are the same number.
    #[test]
    fn m926_r2_vector_matches_the_hackage_tarball_digest() {
        let nix = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
        let expect = "9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff";
        assert_eq!(sha256_to_hex(nix).unwrap(), expect);
    }

    #[test]
    fn m926_decodes_to_exactly_32_bytes() {
        let nix = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
        assert_eq!(nix.len(), 52);
        assert_eq!(decode(nix).unwrap().len(), 32);
    }

    /// Width is a claim about the algorithm, so a wrong width yields nothing.
    #[test]
    fn m926_wrong_width_yields_none() {
        assert!(sha256_to_hex("0123456789").is_none());
        assert!(sha256_to_hex("").is_none());
    }

    /// The alphabet omits `e`, `o`, `u`, `t` — a value containing one is not
    /// Nix base32, and is rejected rather than silently mapped.
    #[test]
    fn m926_characters_outside_the_alphabet_are_rejected() {
        for bad in ['e', 'o', 'u', 't'] {
            let v: String = std::iter::repeat_n('1', 51).chain([bad]).collect();
            assert!(decode(&v).is_none(), "{bad} should be rejected");
        }
    }
}
