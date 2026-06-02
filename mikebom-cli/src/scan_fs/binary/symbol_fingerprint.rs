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
///
/// Milestone 108 adds three optional fields:
/// - `target_purl`: explicit PURL string. Bundled-corpus matches set
///   this to `pkg:generic/<library>` (identical to today's hardcoded
///   format in `symbol_match_to_entry`); external-corpus matches use
///   the record's `target_purl` field (which may carry variant or
///   non-generic namespaces — e.g. `pkg:generic/openssl?variant=fips`).
/// - `corpus_sha_annotation`: when `Some(_)`, the value is emitted as
///   the `mikebom:fingerprint-corpus-sha` annotation per FR-005.
///   `None` means "don't emit the annotation" — preserves SC-003
///   byte-identity for non-opt-in scans.
/// - `also_detected_via`: FR-013 multi-record collision. When two
///   records match the same target binary (vendor-fork variant + the
///   upstream library), each match lists the OTHER's library name
///   here, surfaced as `mikebom:also-detected-via`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolFingerprintMatch {
    pub library: String,
    pub matched_count: usize,
    pub total_count: usize,
    pub target_purl: String,
    pub corpus_sha_annotation: Option<String>,
    pub also_detected_via: Vec<String>,
}

struct SymbolFingerprint {
    library: &'static str,
    symbols: &'static [&'static str],
    required: usize,
}

/// Symbol-fingerprint starter set. Milestone 096 shipped 3 libraries
/// (openssl, zlib, libcurl); milestone 099 expanded to 7 by adding
/// sqlite, pcre, pcre2, gnutls. Each library lists ≥10 public-API
/// symbols; a match fires when ≥80% are present in the binary's
/// `.dynsym` table.
///
/// Documented omissions per milestone-099 spec §1 + FR-004. These
/// libraries are intentionally NOT in the fingerprint table; future
/// maintainers should consult the per-library rationale before adding
/// them:
///
///   - boringssl: drop-in OpenSSL ABI replacement; symbol overlap with
///     openssl prevents reliable disambiguation at the symbol level.
///     Operators wanting fork identification rely on the version-string
///     scanner's `BoringSSL ` anchor (milestone 026).
///   - libressl: same reasoning — OpenBSD's OpenSSL fork shares ABI.
///     Version-string scanner's `LibreSSL ` anchor handles disambiguation.
///   - llvm: API surface too broad (hundreds of public-API entry points
///     across libLLVMCore / libLLVMAnalysis / ...); no stable 10-symbol
///     slice. Different mikebom releases would pick different slices.
///     Defer until a versioned compiler-libs strategy emerges.
///   - openjdk: the launcher binary doesn't statically link JDK APIs
///     (those live in libjvm.so loaded via JNI). Symbol fingerprinting
///     at the launcher level yields no signal. Defer indefinitely.
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
    // Milestone 099 — SQLite. `sqlite3_*` prefix → near-zero collision
    // risk; most-statically-linked C library in CLI tooling. API stable
    // since SQLite 3.0 (2004).
    SymbolFingerprint {
        library: "sqlite",
        symbols: &[
            "sqlite3_open",
            "sqlite3_close",
            "sqlite3_exec",
            "sqlite3_prepare_v2",
            "sqlite3_step",
            "sqlite3_finalize",
            "sqlite3_bind_int",
            "sqlite3_column_text",
            "sqlite3_errmsg",
            "sqlite3_libversion",
        ],
        required: 8,
    },
    // Milestone 099 — PCRE 8.x. `pcre_*` prefix (distinct from `pcre2_*`).
    // API frozen as of PCRE 8.45 (final 8.x release, 2021).
    SymbolFingerprint {
        library: "pcre",
        symbols: &[
            "pcre_compile",
            "pcre_exec",
            "pcre_free",
            "pcre_study",
            "pcre_get_substring",
            "pcre_version",
            "pcre_fullinfo",
            "pcre_compile2",
            "pcre_dfa_exec",
            "pcre_jit_exec",
        ],
        required: 8,
    },
    // Milestone 099 — PCRE 10.x. 8-bit width variant only (`pcre2_*_8`
    // — `libpcre2-8`); 16-bit / 32-bit variants deferred per
    // research §3 (separate `libpcre2-{16,32}` artifacts; same NVD CPE
    // `pcre:pcre2`).
    SymbolFingerprint {
        library: "pcre2",
        symbols: &[
            "pcre2_compile_8",
            "pcre2_match_8",
            "pcre2_match_data_create_8",
            "pcre2_substring_get_byname_8",
            "pcre2_substring_get_bynumber_8",
            "pcre2_get_ovector_pointer_8",
            "pcre2_code_free_8",
            "pcre2_match_data_free_8",
            "pcre2_compile_context_create_8",
            "pcre2_set_compile_extra_options_8",
        ],
        required: 8,
    },
    // Milestone 099 — GnuTLS. `gnutls_*` prefix; Mozilla NSS / OpenSSL
    // alternative common on Debian-derived distros. API stable since
    // GnuTLS 3.0 (2011).
    SymbolFingerprint {
        library: "gnutls",
        symbols: &[
            "gnutls_init",
            "gnutls_deinit",
            "gnutls_handshake",
            "gnutls_record_send",
            "gnutls_record_recv",
            "gnutls_global_init",
            "gnutls_global_deinit",
            "gnutls_set_default_priority",
            "gnutls_credentials_set",
            "gnutls_session_set_ptr",
        ],
        required: 8,
    },
];

/// Milestone 108 — expose the bundled `FINGERPRINTS` const as a
/// `Vec<FingerprintRecord>` for the new `fingerprints::load_bundled`
/// path. `FINGERPRINTS` uses `&'static str` for compile-time
/// efficiency; the bundled-records view allocates owned Strings once
/// per process (memoized via `OnceLock` in
/// `super::fingerprints::load_bundled`).
///
/// The bundled records are SEMANTICALLY IDENTICAL to what the seeded
/// `kusari-sandbox/mikebom-fingerprints` repo ships on day 1 — same
/// 7 libraries, same symbol lists, same `min_symbols=8` threshold.
/// Operators who don't opt into the external corpus see zero
/// behavioral change (FR-001 / SC-003).
///
/// **DO NOT ADD NEW LIBRARIES HERE.** Post-milestone-108, the
/// source-of-truth corpus lives at `kusari-sandbox/mikebom-fingerprints`.
/// This const is the bundled fallback ONLY. (Const-growth guard
/// task T060a — milestone 108 polish PR — adds a unit test asserting
/// `FINGERPRINTS.len() == 7`.)
#[allow(dead_code)]
pub(crate) fn bundled_records() -> Vec<super::fingerprints::FingerprintRecord> {
    FINGERPRINTS
        .iter()
        .map(|fp| super::fingerprints::FingerprintRecord {
            library: fp.library.to_string(),
            target_purl: format!("pkg:generic/{}", fp.library),
            symbols: fp.symbols.iter().map(|s| s.to_string()).collect(),
            min_symbols: fp.required as u32,
            version_hint: None,
            variant: None,
            notes: None,
        })
        .collect()
}

/// Match the binary's dynamic-symbol set against the v1 fingerprint
/// table. Returns one entry per matched library.
///
/// `symbol_names` is a slice of exported-symbol names (the values
/// the caller pulled from ELF `.dynsym`). Empty slice → empty result.
///
/// Milestone 108: this is now a thin wrapper around `scan_with_corpus`
/// that always uses the bundled fallback (`stamp_corpus_sha = false`).
/// New callers should prefer `scan_with_corpus` so they can pass
/// in the active corpus (bundled OR external) and the stamp opt-in.
/// SC-003 byte-identity: `scan()`'s output is identical to the
/// pre-108 implementation — same matches, no new annotations.
pub fn scan(symbol_names: &[String]) -> Vec<SymbolFingerprintMatch> {
    scan_with_corpus(symbol_names, super::fingerprints::load_bundled(), false)
}

/// Match the binary's dynamic-symbol set against a (bundled OR
/// external) corpus. Drives the milestone-108 opt-in path.
///
/// When `stamp_corpus_sha` is true, every emitted match carries the
/// `corpus_sha_annotation` set from `corpus.source.annotation_value()`
/// (12-hex for `Cached`/`Fetched`, literal `"bundled"` for the
/// fallback path). When false, the annotation is `None` — preserves
/// SC-003 byte-identity for the bundled-only path.
///
/// FR-013 multi-record collision: when ≥2 records in `corpus.records`
/// both clear their `min_symbols` threshold against `symbol_names`,
/// each emitted match's `also_detected_via` lists the OTHER matching
/// records' library names (alphabetically sorted for byte-stable
/// downstream emission).
pub fn scan_with_corpus(
    symbol_names: &[String],
    corpus: &super::fingerprints::FingerprintCorpus,
    stamp_corpus_sha: bool,
) -> Vec<SymbolFingerprintMatch> {
    if symbol_names.is_empty() {
        return Vec::new();
    }
    let symbol_set: std::collections::HashSet<&str> =
        symbol_names.iter().map(String::as_str).collect();
    let corpus_annotation = if stamp_corpus_sha {
        Some(corpus.source.annotation_value())
    } else {
        None
    };

    // First pass: collect every record whose match-count clears its
    // `min_symbols` threshold. For FR-013 collision detection (second
    // pass) we also keep the set of symbols that drove each record's
    // match — the cheapest unambiguous discriminator between
    // "co-resident independent libraries" (disjoint matched sets;
    // common case — openssl + zlib in the same binary) and
    // "variant collision" (overlapping matched sets — LibreSSL and
    // OpenSSL records both satisfied by the same `SSL_*` symbols).
    let mut hits: Vec<SymbolFingerprintMatch> = Vec::new();
    let mut matched_symbol_sets: Vec<std::collections::HashSet<String>> = Vec::new();
    for record in &corpus.records {
        let matched_symbols: std::collections::HashSet<String> = record
            .symbols
            .iter()
            .filter(|sym| symbol_set.contains(sym.as_str()))
            .cloned()
            .collect();
        if matched_symbols.len() as u32 >= record.min_symbols {
            hits.push(SymbolFingerprintMatch {
                library: record.library.clone(),
                matched_count: matched_symbols.len(),
                total_count: record.symbols.len(),
                target_purl: record.target_purl.clone(),
                corpus_sha_annotation: corpus_annotation.clone(),
                also_detected_via: Vec::new(),
            });
            matched_symbol_sets.push(matched_symbols);
        }
    }
    // Second pass: FR-013 — populate `also_detected_via` ONLY for
    // hits whose matched-symbol sets overlap (variant collision).
    // Independent co-resident libraries have disjoint matched sets and
    // do NOT trigger this annotation — preserves SC-003 byte-identity
    // for the common multi-library case (e.g., openssl + zlib in one
    // binary, exercised by existing `two_libraries_both_match` test).
    if hits.len() > 1 {
        for i in 0..hits.len() {
            let mut others: Vec<String> = Vec::new();
            for (j, other_hit) in hits.iter().enumerate() {
                if i == j {
                    continue;
                }
                let overlap = matched_symbol_sets[i]
                    .intersection(&matched_symbol_sets[j])
                    .count();
                if overlap > 0 {
                    others.push(other_hit.library.clone());
                }
            }
            others.sort();
            hits[i].also_detected_via = others;
        }
    }
    hits
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
            hits.iter().map(|h| h.library.as_str()).collect();
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

    // ====================================================================
    // Milestone 099 — symbol-fingerprint expansion
    // (sqlite, pcre, pcre2, gnutls)
    // ====================================================================

    /// T006 — SQLite full set matches (FR-001 / SC-001 / Contract 1).
    /// Distinctive `sqlite3_*` prefix; all 10 symbols → one match.
    #[test]
    fn sqlite_full_set_matches() {
        let s = syms(&[
            "sqlite3_open",
            "sqlite3_close",
            "sqlite3_exec",
            "sqlite3_prepare_v2",
            "sqlite3_step",
            "sqlite3_finalize",
            "sqlite3_bind_int",
            "sqlite3_column_text",
            "sqlite3_errmsg",
            "sqlite3_libversion",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "sqlite");
        assert_eq!(hits[0].matched_count, 10);
        assert_eq!(hits[0].total_count, 10);
    }

    /// T007 — SQLite under-threshold guard (FR-007 / Contract 2).
    /// Only 7 of 10 symbols → under the 8/10 threshold → no match.
    /// The threshold logic is shared across all libraries, so this one
    /// test guards the threshold gate for all 7 fingerprints uniformly.
    #[test]
    fn sqlite_seven_of_ten_below_threshold() {
        let s = syms(&[
            "sqlite3_open",
            "sqlite3_close",
            "sqlite3_exec",
            "sqlite3_prepare_v2",
            "sqlite3_step",
            "sqlite3_finalize",
            "sqlite3_bind_int",
        ]);
        assert!(scan(&s).is_empty());
    }

    /// T010 — PCRE 8.x full set matches (FR-001 / SC-002 / Contract 3).
    /// `pcre_*` prefix; the test asserts the matched library is `pcre`
    /// (NOT `pcre2`) — the `hits.len() == 1` + library-name check
    /// jointly verify the 8.x / 10.x disambiguation per Contract 3.
    #[test]
    fn pcre_full_set_matches() {
        let s = syms(&[
            "pcre_compile",
            "pcre_exec",
            "pcre_free",
            "pcre_study",
            "pcre_get_substring",
            "pcre_version",
            "pcre_fullinfo",
            "pcre_compile2",
            "pcre_dfa_exec",
            "pcre_jit_exec",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "pcre");
        assert_eq!(hits[0].matched_count, 10);
    }

    /// T011 — PCRE 10.x (8-bit width) full set matches.
    /// `pcre2_*_8` prefix; asserts the matched library is `pcre2`
    /// (NOT `pcre`) — same disambiguation guarantee as T010.
    #[test]
    fn pcre2_full_set_matches() {
        let s = syms(&[
            "pcre2_compile_8",
            "pcre2_match_8",
            "pcre2_match_data_create_8",
            "pcre2_substring_get_byname_8",
            "pcre2_substring_get_bynumber_8",
            "pcre2_get_ovector_pointer_8",
            "pcre2_code_free_8",
            "pcre2_match_data_free_8",
            "pcre2_compile_context_create_8",
            "pcre2_set_compile_extra_options_8",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "pcre2");
        assert_eq!(hits[0].matched_count, 10);
    }

    /// T014 — GnuTLS full set matches (FR-001 / SC-003).
    /// `gnutls_*` prefix.
    #[test]
    fn gnutls_full_set_matches() {
        let s = syms(&[
            "gnutls_init",
            "gnutls_deinit",
            "gnutls_handshake",
            "gnutls_record_send",
            "gnutls_record_recv",
            "gnutls_global_init",
            "gnutls_global_deinit",
            "gnutls_set_default_priority",
            "gnutls_credentials_set",
            "gnutls_session_set_ptr",
        ]);
        let hits = scan(&s);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, "gnutls");
        assert_eq!(hits[0].matched_count, 10);
    }

    // ====================================================================
    // Milestone 108 — annotation stamping + FR-013 multi-record collision
    // ====================================================================

    /// T039 — bundled, non-opt-in `scan()` does NOT stamp the corpus-sha
    /// annotation (`corpus_sha_annotation == None`). Preserves SC-003
    /// byte-identity for the 33 byte-identity goldens.
    #[test]
    fn scan_emits_no_corpus_sha_annotation_for_bundled_non_opt_in() {
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
        assert_eq!(hits[0].corpus_sha_annotation, None);
        // target_purl is still populated (cheap; identical to the
        // pre-108 hardcoded `pkg:generic/<library>` shape).
        assert_eq!(hits[0].target_purl, "pkg:generic/openssl");
    }

    /// T039 — bundled corpus + opt-in stamps the `bundled` sentinel
    /// per FR-005's "bundled" sentinel rule.
    #[test]
    fn scan_with_corpus_emits_bundled_sentinel_for_bundled_opt_in() {
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
        let bundled = super::super::fingerprints::load_bundled();
        let hits = scan_with_corpus(&s, bundled, true);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].corpus_sha_annotation.as_deref(),
            Some("bundled"),
        );
    }

    /// T039 — external `Cached` corpus stamps the 12-hex SHA per FR-005.
    #[test]
    fn scan_with_corpus_emits_12_hex_for_cached_corpus() {
        use super::super::fingerprints::{
            CorpusSha, CorpusSource, FingerprintCorpus, FingerprintRecord,
        };
        let sha = CorpusSha::from_hex("fff39c6ad22ce8420b506323ce1d5cce4b628d5c").unwrap();
        let corpus = FingerprintCorpus {
            records: vec![FingerprintRecord {
                library: "libfoo".to_string(),
                target_purl: "pkg:generic/libfoo".to_string(),
                symbols: vec![
                    "foo_init".to_string(),
                    "foo_run".to_string(),
                    "foo_close".to_string(),
                    "foo_a".to_string(),
                    "foo_b".to_string(),
                ],
                min_symbols: 3,
                version_hint: None,
                variant: None,
                notes: None,
            }],
            source: CorpusSource::Cached { sha },
        };
        let s = syms(&["foo_init", "foo_run", "foo_close"]);
        let hits = scan_with_corpus(&s, &corpus, true);
        assert_eq!(hits.len(), 1);
        let stamp = hits[0].corpus_sha_annotation.as_deref().unwrap();
        assert_eq!(stamp.len(), 12, "annotation must be 12-hex; got {stamp:?}");
        assert_eq!(stamp, "fff39c6ad22c");
    }

    /// T038a — FR-013 multi-record collision: two records that match
    /// the same target binary via overlapping symbol sets BOTH emit,
    /// and each carries the OTHER's library name in `also_detected_via`.
    /// Models the LibreSSL / OpenSSL variant-collision case.
    #[test]
    fn multi_record_match_emits_both_components_with_also_detected_via() {
        use super::super::fingerprints::{
            CorpusSha, CorpusSource, FingerprintCorpus, FingerprintRecord,
        };
        let sha = CorpusSha::from_hex("fff39c6ad22ce8420b506323ce1d5cce4b628d5c").unwrap();
        let shared_symbols: Vec<String> = (0..5).map(|i| format!("SSL_shared_{i}")).collect();
        let corpus = FingerprintCorpus {
            records: vec![
                FingerprintRecord {
                    library: "openssl".to_string(),
                    target_purl: "pkg:generic/openssl".to_string(),
                    symbols: shared_symbols.clone(),
                    min_symbols: 3,
                    version_hint: None,
                    variant: None,
                    notes: None,
                },
                FingerprintRecord {
                    library: "libressl".to_string(),
                    target_purl: "pkg:generic/openssl?variant=libressl".to_string(),
                    symbols: shared_symbols.clone(),
                    min_symbols: 3,
                    version_hint: None,
                    variant: Some("libressl".to_string()),
                    notes: None,
                },
            ],
            source: CorpusSource::Cached { sha },
        };
        let hits = scan_with_corpus(&shared_symbols, &corpus, true);
        assert_eq!(hits.len(), 2, "both records must emit (no silent dedup)");
        // openssl → also_detected_via = ["libressl"]
        let openssl = hits.iter().find(|h| h.library == "openssl").unwrap();
        assert_eq!(openssl.also_detected_via, vec!["libressl".to_string()]);
        // libressl → also_detected_via = ["openssl"]
        let libressl = hits.iter().find(|h| h.library == "libressl").unwrap();
        assert_eq!(libressl.also_detected_via, vec!["openssl".to_string()]);
        // Both also carry the corpus-sha stamp.
        assert!(openssl.corpus_sha_annotation.is_some());
        assert!(libressl.corpus_sha_annotation.is_some());
    }

    /// T038a — independent co-resident libraries (openssl + zlib with
    /// disjoint symbol sets) do NOT trigger `also_detected_via`.
    /// SC-003 byte-identity guard: this is the common case for the
    /// existing 33 byte-identity goldens (multi-library binaries).
    #[test]
    fn independent_libraries_do_not_emit_also_detected_via() {
        let s = syms(&[
            // OpenSSL — 8 symbols.
            "OPENSSL_init_ssl",
            "OPENSSL_init_crypto",
            "SSL_CTX_new",
            "SSL_library_init",
            "EVP_DigestInit_ex",
            "EVP_EncryptInit_ex",
            "RSA_new",
            "BN_new",
            // zlib — 8 symbols. Disjoint from the openssl set.
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
        assert_eq!(hits.len(), 2);
        for h in &hits {
            assert!(
                h.also_detected_via.is_empty(),
                "{} should have empty also_detected_via (disjoint matched sets)",
                h.library
            );
        }
    }

    /// T060a — SC-001 const-growth guard.
    ///
    /// **DO NOT BUMP THIS NUMBER unless you've actually intended to
    /// add to the BUNDLED fallback.** Post-milestone-108, the source-
    /// of-truth fingerprint corpus lives at
    /// `kusari-sandbox/mikebom-fingerprints`. New libraries go in the
    /// sibling repo (via a PR there), not in this const. The bundled
    /// const stays at 7 entries as the offline-only fallback that
    /// every mikebom-cli release ships with, regardless of cache
    /// state.
    ///
    /// If a release intentionally raises the floor (e.g., milestone
    /// 200 adds 3 ubiquitous libraries to the bundled set so they're
    /// identified even without `--fingerprints-corpus`), update both
    /// the assertion AND the doc-comment at the top of `FINGERPRINTS`
    /// to keep the intent visible. The lift is the doc update — the
    /// number bump itself is one character.
    #[test]
    fn bundled_fingerprint_const_size_locked() {
        assert_eq!(
            FINGERPRINTS.len(),
            7,
            "FINGERPRINTS grew beyond the 7-library bundled-fallback floor. \
             New fingerprints belong in `kusari-sandbox/mikebom-fingerprints`, \
             not in this const. See the doc comment on FINGERPRINTS + this \
             test's docstring for the lift required to legitimately raise \
             the floor."
        );
    }
}
