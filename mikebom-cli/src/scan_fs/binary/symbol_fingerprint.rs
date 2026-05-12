//! Symbol-fingerprint scanner. Milestone 096 US3 / FR-004.
//!
//! When a binary statically links a library but has its embedded
//! version string stripped (or never embedded one), the exported-symbol
//! table is the last static-link signal we have. Public-API symbols
//! like `OPENSSL_init_ssl` or `curl_easy_perform` are stable across
//! ten years of releases and rarely appear coincidentally in other
//! libraries — a binary that exports 8 of OpenSSL's 10 well-known
//! public symbols almost certainly contains OpenSSL.
//!
//! v1 starter set (research §3): 3 libraries × 10 symbols each, 8/10
//! match threshold. ELF-only; PE export-table + Mach-O `LC_DYSYMTAB`
//! fingerprinting are deferred per spec Out-of-Scope.
//!
//! Confidence is intentionally lower than embedded-version-string
//! (0.4 vs 0.6) because symbol presence alone can't pin a version —
//! `OPENSSL_init_ssl` ships in every OpenSSL 1.1.0+ release.

/// One match from the fingerprint scanner. Converted to a
/// `PackageDbEntry` with `pkg:generic/<library>` (no `@version`),
/// `mikebom:evidence-kind = "symbol-fingerprint"`,
/// `mikebom:confidence = "heuristic"`, and
/// `mikebom:fingerprint-symbols-matched = "<count>/<total>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolFingerprintMatch {
    pub library: &'static str,
    pub matched_count: usize,
    pub total_count: usize,
}

struct SymbolFingerprint {
    library: &'static str,
    symbols: &'static [&'static str],
    required: usize,
}

/// v1 starter set per research §3. Each library lists ≥10 public-API
/// symbols; a match fires when ≥80% are present in the binary's
/// `.dynsym` table.
const FINGERPRINTS: &[SymbolFingerprint] = &[
    SymbolFingerprint {
        library: "openssl",
        symbols: &[
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
            "BN_new",
            "X509_new",
            "ERR_get_error",
        ],
        required: 8,
    },
    SymbolFingerprint {
        library: "zlib",
        symbols: &[
            "deflate",
            "inflate",
            "deflateInit_",
            "inflateInit_",
            "deflateEnd",
            "inflateEnd",
            "crc32",
            "adler32",
            "compress",
            "uncompress",
        ],
        required: 8,
    },
    SymbolFingerprint {
        library: "libcurl",
        symbols: &[
            "curl_easy_init",
            "curl_easy_setopt",
            "curl_easy_perform",
            "curl_easy_cleanup",
            "curl_easy_getinfo",
            "curl_multi_init",
            "curl_multi_add_handle",
            "curl_global_init",
            "curl_version",
            "curl_slist_append",
        ],
        required: 8,
    },
];

/// Match the binary's dynamic-symbol set against the v1 fingerprint
/// table. Returns one entry per matched library.
///
/// `symbol_names` is a slice of exported-symbol names (the values
/// the caller pulled from ELF `.dynsym`). Empty slice → empty result.
pub fn scan(symbol_names: &[String]) -> Vec<SymbolFingerprintMatch> {
    if symbol_names.is_empty() {
        return Vec::new();
    }
    let symbol_set: std::collections::HashSet<&str> =
        symbol_names.iter().map(String::as_str).collect();

    let mut out = Vec::new();
    for fp in FINGERPRINTS {
        let matched = fp
            .symbols
            .iter()
            .filter(|sym| symbol_set.contains(**sym))
            .count();
        if matched >= fp.required {
            out.push(SymbolFingerprintMatch {
                library: fp.library,
                matched_count: matched,
                total_count: fp.symbols.len(),
            });
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn syms(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_input_no_matches() {
        assert!(scan(&[]).is_empty());
    }

    #[test]
    fn openssl_full_set_matches() {
        let s = syms(&[
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
            "BN_new",
            "X509_new",
            "ERR_get_error",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "openssl");
        assert_eq!(hits[0].matched_count, 10);
        assert_eq!(hits[0].total_count, 10);
    }

    #[test]
    fn openssl_eight_of_ten_just_matches() {
        // 8 of 10 = exactly at threshold.
        let s = syms(&[
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
            "BN_new",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_count, 8);
    }

    #[test]
    fn openssl_seven_of_ten_below_threshold() {
        // 7 of 10 = below threshold → no match.
        let s = syms(&[
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
        ]);
        assert!(scan(&s).is_empty());
    }

    #[test]
    fn zlib_matches() {
        let s = syms(&[
            "deflate",
            "inflate",
            "deflateInit_",
            "inflateInit_",
            "deflateEnd",
            "inflateEnd",
            "crc32",
            "adler32",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "zlib");
        assert_eq!(hits[0].matched_count, 8);
    }

    #[test]
    fn libcurl_matches_at_threshold() {
        let s = syms(&[
            "curl_easy_init",
            "curl_easy_setopt",
            "curl_easy_perform",
            "curl_easy_cleanup",
            "curl_easy_getinfo",
            "curl_multi_init",
            "curl_multi_add_handle",
            "curl_global_init",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "libcurl");
        assert_eq!(hits[0].matched_count, 8);
    }

    #[test]
    fn unrelated_symbols_no_match() {
        // Random kernel/glibc-style symbols, none overlapping with the
        // v1 fingerprint set.
        let s = syms(&[
            "main",
            "printf",
            "malloc",
            "free",
            "strcpy",
            "strlen",
            "memcpy",
            "open",
            "close",
            "read",
        ]);
        assert!(scan(&s).is_empty());
    }

    #[test]
    fn two_libraries_both_match() {
        // OpenSSL + zlib symbols co-resident in one symbol table.
        let mut s = syms(&[
            // OpenSSL — 8 symbols.
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
            "BN_new",
        ]);
        s.extend(syms(&[
            // zlib — 8 symbols.
            "deflate",
            "inflate",
            "deflateInit_",
            "inflateInit_",
            "deflateEnd",
            "inflateEnd",
            "crc32",
            "adler32",
        ]));
        let hits = scan(&s);
        assert_eq!(hits.len(), 2);
        let libs: std::collections::HashSet<&str> =
            hits.iter().map(|h| h.library).collect();
        assert!(libs.contains("openssl"));
        assert!(libs.contains("zlib"));
    }

    #[test]
    fn duplicate_symbols_dont_double_count() {
        // HashSet dedup means listing the same symbol twice still
        // counts as one match — guards against accidental
        // multi-versioned-symbol-table double-counting.
        let s = syms(&[
            "deflate",
            "deflate",
            "inflate",
            "deflateInit_",
            "inflateInit_",
            "deflateEnd",
            "inflateEnd",
            "crc32",
            "adler32",
        ]);
        let hits = scan(&s);
        // 8 distinct zlib symbols → matches.
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_count, 8);
    }
}
