//! #1046 — the store path of a fixed-output derivation's output.
//!
//! `nix derivation show` format 4 records a fixed-output derivation's output
//! as `{"hash", "method"}` and **no `path`**: the path is a function of the
//! hash, the ingestion method and the name, so nix omits it. Every fetch is a
//! fixed-output derivation, so an index keyed by recorded output paths holds
//! no fetch at all. Measured at nixpkgs `5b2c2d84` with Nix 2.34: all 386
//! fixed-output derivations in `haskellPackages.hspec-discover`'s closure
//! carry `hash` and `method`, none carries `path`.
//!
//! The computation is nix's `makeFixedOutputPath`. Only SHA-256 with the
//! `nar` or `flat` method is computed; any other algorithm or method yields
//! `None` (Principle IX: no path rather than a guessed one).

use data_encoding::{BASE64, HEXLOWER};
use sha2::{Digest, Sha256};

const NIX_BASE32: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// The 32-character hash part of the output path of a fixed-output
/// derivation named `name`, under `store_dir` (`/nix/store`), whose output
/// records `method` and the SRI `hash`.
pub(crate) fn path_hash(store_dir: &str, name: &str, method: &str, hash: &str) -> Option<String> {
    let digest = BASE64.decode(hash.strip_prefix("sha256-")?.as_bytes()).ok()?;
    if digest.len() != 32 {
        return None;
    }
    let hex = HEXLOWER.encode(&digest);
    let fingerprint = match method {
        "nar" => format!("source:sha256:{hex}:{store_dir}:{name}"),
        "flat" => {
            let inner = HEXLOWER.encode(&Sha256::digest(format!("fixed:out:sha256:{hex}:")));
            format!("output:out:sha256:{inner}:{store_dir}:{name}")
        }
        _ => return None,
    };
    let full = Sha256::digest(fingerprint.as_bytes());
    let mut compressed = [0u8; 20];
    for (i, b) in full.iter().enumerate() {
        compressed[i % 20] ^= b;
    }
    Some(encode_base32(&compressed))
}

/// Nix base32: the alphabet omits `e o u t`, and the first character encodes
/// the most significant bits of the *last* byte.
fn encode_base32(bytes: &[u8]) -> String {
    let len = (bytes.len() * 8 - 1) / 5 + 1;
    (0..len)
        .rev()
        .map(|n| {
            let b = n * 5;
            let (i, j) = (b / 8, b % 8);
            let lo = u16::from(bytes[i]) >> j;
            let hi = bytes.get(i + 1).map_or(0, |&x| u16::from(x) << (8 - j));
            char::from(NIX_BASE32[usize::from((lo | hi) & 0x1f)])
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Real vectors from nixpkgs `5b2c2d84`, each checked against both
    /// `nix-store --query --outputs <drv>` and `nix-store --print-fixed-path`.
    #[test]
    fn real_fixed_output_paths() {
        assert_eq!(
            path_hash(
                "/nix/store",
                "hspec-discover-2.11.16.tar.gz",
                "flat",
                "sha256-vWzPHg0h1Cs8eAi7e0Sa6F3FfDnqrGjy9eo31ZcRj6w="
            )
            .unwrap(),
            "72m2wa52ycqhzy7xapnw0m30p3vh0wn4"
        );
        assert_eq!(
            path_hash(
                "/nix/store",
                "source",
                "nar",
                "sha256-j5Ep1RX5DTJqTGszrF4d/JtzUqZ6nA6XoExqcIQ0RVQ="
            )
            .unwrap(),
            "0rwfc8gn35h2x49v9s9v0bglv2ycyvb9"
        );
    }

    /// The method changes the path: the same hash and name under `flat` and
    /// `nar` are different store objects.
    #[test]
    fn the_method_is_part_of_the_path() {
        let h = "sha256-j5Ep1RX5DTJqTGszrF4d/JtzUqZ6nA6XoExqcIQ0RVQ=";
        assert_ne!(path_hash("/nix/store", "source", "flat", h), path_hash("/nix/store", "source", "nar", h));
    }

    #[test]
    fn anything_else_is_not_computed() {
        let h = "sha256-j5Ep1RX5DTJqTGszrF4d/JtzUqZ6nA6XoExqcIQ0RVQ=";
        assert_eq!(path_hash("/nix/store", "source", "git", h), None);
        assert_eq!(path_hash("/nix/store", "source", "text", h), None);
        assert_eq!(path_hash("/nix/store", "source", "nar", "sha1-AAAAAAAAAAAAAAAAAAAAAAAAAAA="), None);
        assert_eq!(path_hash("/nix/store", "source", "nar", "sha256-AAAA"), None);
        assert_eq!(path_hash("/nix/store", "source", "nar", "sha256-!!"), None);
    }
}
