//! Heuristic CPE 2.3 synthesizer — syft-style multi-candidate emission.
//!
//! No authoritative source of CPE identifiers exists for OS-distributed
//! packages at scale. NVD's CPE Dictionary has ~1M entries and the
//! vendor slug for any given package varies (e.g. the `jq` tool lives
//! under `cpe:2.3:a:jqlang:jq:...` in NVD today, under
//! `cpe:2.3:a:jq_project:jq:...` historically, and syft sometimes
//! synthesizes `cpe:2.3:a:debian:jq:...` because that's what the
//! install metadata points at). We follow syft's approach: emit
//! **multiple candidates** per component so a downstream matcher can
//! take the union against NVD and find any hit that exists.
//!
//! Format reference: CPE 2.3 formatted-string binding
//! <https://nvlpubs.nist.gov/nistpubs/Legacy/IR/nistir7695.pdf>

use waybill_common::resolution::ResolvedComponent;

/// v1 mapping table for `pkg:generic/<library>@<version>` components per
/// milestone-097 FR-001. Each row maps a library slug (the lowercase
/// value emitted by `version_strings::CuratedLibrary::slug()` and
/// `symbol_fingerprint::SymbolFingerprintMatch::library`) to one or more
/// `(NVD vendor, NVD product)` pairs.
///
/// Sorted alphabetically by library_slug for diff-friendliness per FR-002.
/// Adding a library is a one-line PR; new entries must keep the sort.
///
/// BoringSSL is intentionally omitted, and the reason is **not** that
/// NVD has no namespace for it. It does: probed on 2026-10-24,
/// `cpe:2.3:a:google:boringssl` matched 2 dictionary entries, a keyword
/// search for `boringssl` returned those same 2 and nothing else, and
/// `virtualMatchString=cpe:2.3:a:google:boringssl:*:*:*:*:*:*:*:*`
/// returned one advisory, CVE-2018-12440 (positive control:
/// `openssl:openssl:3.0.11` returned 41). This comment claimed the
/// opposite until #1268.
///
/// The reason is that BoringSSL has no release versioning. Its
/// signature is `BoringSSL <40-char-sha>`, so the version waybill
/// extracts is a git revision (`parse_git_sha` in
/// `scan_fs/binary/version_strings.rs` accepts exactly 40 lowercase hex
/// bytes and nothing else). A CPE's version component is a release
/// string that a matcher orders against advisory ranges; a git SHA
/// cannot be ordered against anything, so a `google:boringssl` CPE
/// built from one matches either nothing or arbitrarily. That is the
/// same hazard this table already refuses when it declines
/// wildcard-version CPEs, which match a library's whole history.
///
/// **The general rule this row stands for**: a library whose only
/// observable version is not a release identifier gets no CPE,
/// whether or not a namespace exists for it. The namespace question is
/// settled by `scripts/probe-cpe-dictionary.sh`; it is necessary and
/// not sufficient. The inverse reading — "I could not find an NVD
/// namespace, so I left it out" — is the unverified assumption the
/// probe exists to prevent, and is how upstream shipped `expat:expat`
/// (anchore/syft#4788) and display-name CPEs (anchore/syft#5429).
///
/// Components for the `boringssl` slug therefore emit a PURL with no
/// CPE field — an explicit "we don't know" per Constitution X
/// transparency. BoringSSL-rooted vulnerabilities still reach a
/// consumer through `openssl:openssl` records where the issue exists
/// upstream.
///
/// The distinction is not carried in the SBOM, and #1268 asked whether
/// `waybill:unresolved-reason` (C151) should carry it. It cannot: C151
/// is emitted if and only if the component carries
/// `waybill:sbom-tier: "design"`, and a version-string-scanner
/// component is binary tier. Saying "this product's versioning cannot
/// key a CPE" on the wire needs a carrier that applies to a resolved
/// component, which is a separate question from this row's rationale.
const GENERIC_LIBRARY_CPES: &[(&str, &[(&str, &str)])] = &[
    // libcurl: historical `haxx:curl` dominates pre-2023 NVD records;
    // modern `curl:curl` also appears. Both emitted — primary goes to
    // `component.cpe`, secondary to `waybill:cpe-candidates` property.
    ("curl", &[("haxx", "curl"), ("curl", "curl")]),
    // GnuTLS — NVD canonical.
    ("gnutls", &[("gnu", "gnutls")]),
    // LibreSSL — `openbsd:libressl` (LibreSSL is an OpenBSD project).
    // A second `libressl:libressl` candidate was emitted here until
    // #1246: the vendor does not exist. All 110 libressl entries in
    // NVD's dictionary are `openbsd:libressl` and none use a `libressl`
    // vendor, so the candidate could only ever return zero.
    ("libressl", &[("openbsd", "libressl")]),
    // LLVM umbrella project. Sub-projects (clang, lld) are out of scope
    // for v1 — the version-string scanner doesn't distinguish them.
    ("llvm", &[("llvm", "llvm")]),
    // Node.js (issue #1244) — the product segment is `node.js`, not
    // `nodejs`. This is corroborated in-repo: the milestone-128 Yocto
    // recipe-to-CPE map, sourced from openembedded-core's own CVE
    // exclusions, carries the same `("nodejs", "node.js")` pair at
    // `scan_fs/package_db/yocto/cpe_name_map.rs`. Vendor is `nodejs`.
    //
    // This maps the RUNTIME only. Node's bundled V8 and OpenSSL are
    // separate CPEs; OpenSSL already has its own row below and is
    // matched independently by the version-string scanner.
    ("node", &[("nodejs", "node.js")]),
    // OpenJDK — `oracle:openjdk` dominates Java vuln records. Build
    // suffix (e.g. `+12`) is stripped before CPE emission per NVD-shape
    // inconsistency; the PURL preserves the full version verbatim.
    ("openjdk", &[("oracle", "openjdk")]),
    // OpenSSL — every CVE since 2014 uses `openssl:openssl`.
    ("openssl", &[("openssl", "openssl")]),
    // PCRE 8.x — `pcre:pcre`.
    ("pcre", &[("pcre", "pcre")]),
    // PCRE 10.x — same vendor, different product.
    ("pcre2", &[("pcre", "pcre2")]),
    // PHP — NVD files every interpreter CVE under `php:php`; verified
    // against the configurations on CVE-2026-109618, whose cpeMatch
    // criteria are `cpe:2.3:a:php:php:*`. #1258.
    ("php", &[("php", "php")]),
    // SQLite — NVD canonical.
    ("sqlite", &[("sqlite", "sqlite")]),
    // zlib — NVD canonical.
    ("zlib", &[("zlib", "zlib")]),
];

/// What NVD's CPE dictionary held for every pair in
/// `GENERIC_LIBRARY_CPES`, as `(vendor, product, entries)`.
///
/// Produced by `scripts/probe-cpe-dictionary.sh` on 2026-10-24 and
/// recorded here so `every_table_pair_exists_in_the_nvd_dictionary` can
/// enforce it without a network call. A row may be added to the table
/// above only once it has been probed and added here (#1246).
///
/// WHY. A wrong vendor or product fails *silently*: the matcher finds
/// nothing, which is indistinguishable from a component with no known
/// vulnerabilities. Upstream ships this defect regularly — syft derived
/// `expat:expat` from a Conan recipe name where NVD files the records
/// under `libexpat` (anchore/syft#4788), and built Apple-bundle CPEs
/// from display names, so `Google Chrome` matched nothing where
/// `google:chrome` matches thousands (anchore/syft#5429). The first run
/// of the probe found one here: `libressl:libressl`, removed above.
///
/// The counts are the observation, not an invariant — NVD's dictionary
/// grows, and the test asserts only that a pair was seen at all. What
/// the test deliberately does NOT assert is that a pair carries any
/// advisories: dictionary presence and advisory coverage are different
/// questions, and treating a zero as an answer to the second is the trap
/// recorded in #1245. `probe-cpe-dictionary.sh --cves` asks that one.
#[cfg(test)]
const NVD_DICTIONARY_OBSERVED: &[(&str, &str, u32)] = &[
    ("curl", "curl", 2),
    ("gnu", "gnutls", 366),
    ("haxx", "curl", 228),
    ("llvm", "llvm", 55),
    ("nodejs", "node.js", 1714),
    ("openbsd", "libressl", 110),
    ("openssl", "openssl", 368),
    ("oracle", "openjdk", 241),
    ("pcre", "pcre", 91),
    ("pcre", "pcre2", 25),
    ("php", "php", 1641),
    ("sqlite", "sqlite", 372),
    ("zlib", "zlib", 84),
];

/// Build the set of CPE 2.3 candidate strings for a resolved component.
/// Returns empty when the component is in an ecosystem the synthesizer
/// has no opinion on (generic/unknown PURLs).
pub fn synthesize_cpes(component: &ResolvedComponent) -> Vec<String> {
    let ecosystem = component.purl.ecosystem();
    let name = &component.name;
    let version = &component.version;
    if name.is_empty() || version.is_empty() {
        return Vec::new();
    }

    // Build a deduped, ordered vendor candidate list per ecosystem.
    // Ordering matters — the first candidate is emitted as the primary
    // `component.cpe` in CycloneDX; the rest live in a property list.
    let mut vendors: Vec<String> = Vec::new();
    match ecosystem {
        "deb" => {
            push_unique(&mut vendors, "debian");
            push_unique(&mut vendors, name);
        }
        "apk" => {
            push_unique(&mut vendors, "alpinelinux");
            push_unique(&mut vendors, name);
        }
        "rpm" => {
            // RPM PURLs: `pkg:rpm/<vendor>/<name>@...`. The namespace
            // is already the vendor slug (`redhat`, `rocky`, ...), so
            // emit both that and the bare name — NVD references vary.
            if let Some(namespace) = component.purl.namespace() {
                push_unique(&mut vendors, namespace);
            }
            push_unique(&mut vendors, name);
        }
        "gem" => {
            push_unique(&mut vendors, name);
        }
        "cargo" => {
            // Crates rarely match NVD entries, but when they do the
            // vendor is either the crate name or the crate author —
            // deps.dev-driven enrichment can correct this later.
            push_unique(&mut vendors, name);
        }
        "npm" => {
            push_unique(&mut vendors, name);
            // Scoped packages (@org/pkg) — parse the scope out of the
            // PURL namespace so we emit a candidate under that org too.
            if let Some(namespace) = component.purl.namespace() {
                let scope = namespace.trim_start_matches('@');
                if !scope.is_empty() && scope != name {
                    push_unique(&mut vendors, scope);
                }
            }
        }
        "pypi" => {
            push_unique(&mut vendors, name);
            // NVD commonly namespaces Python packages as `python-<name>`.
            push_unique(&mut vendors, &format!("python-{name}"));
        }
        "golang" | "go" => {
            // Issue #364 — the synthetic `pkg:golang/stdlib` component
            // emitted for every Go-source scan carries the Go toolchain
            // version, which NVD lists under `cpe:2.3:a:golang:go:<v>`.
            // Special-case the vendor + product so consumers can match
            // Go stdlib CVEs (e.g. CVE-2024-34156 big.Int overflow)
            // directly from the synthesized CPE rather than relying on
            // post-processing.
            if name == "stdlib" {
                // The version string carries the `v`-prefix from the
                // PURL (`pkg:golang/stdlib@v1.21.5`); strip it so the
                // CPE matches NVD's bare-version convention.
                let bare_version = version.trim_start_matches('v');
                return vec![format!(
                    "cpe:2.3:a:golang:go:{bare_version}:*:*:*:*:*:*:*"
                )];
            }
            push_unique(&mut vendors, name);
            if let Some(namespace) = component.purl.namespace() {
                push_unique(&mut vendors, namespace);
            }
        }
        "maven" => {
            // Maven PURLs carry groupId as namespace; that's often a
            // reverse-DNS string (com.example.foo) which maps poorly to
            // NVD vendor slugs. Best-effort: emit both the groupId and
            // its final segment (the common case: "org.apache.commons"
            // → `apache`).
            if let Some(namespace) = component.purl.namespace() {
                push_unique(&mut vendors, namespace);
                if let Some(tail) = namespace.rsplit('.').next() {
                    if !tail.is_empty() && tail != namespace {
                        push_unique(&mut vendors, tail);
                    }
                }
            }
            push_unique(&mut vendors, name);
        }
        "nuget" => {
            push_unique(&mut vendors, name);
        }
        "generic" => {
            // Milestone 097 — CPE candidates for binary-extracted
            // `pkg:generic/<lib>@<version>` components from the
            // milestone-096 version-string + symbol-fingerprint
            // scanners. Lookup the lowercase library slug in the
            // hand-curated mapping table; libraries with no row emit
            // no CPE (FR-003 silent-skip) so non-tracked generic PURLs
            // (e.g. `pkg:generic/weird@1.0.0`) continue to produce an
            // empty Vec just as they did before milestone 097.
            let mapping = GENERIC_LIBRARY_CPES
                .iter()
                .find(|(slug, _)| *slug == name.as_str())
                .map(|(_, pairs)| *pairs);
            let Some(pairs) = mapping else {
                return Vec::new();
            };
            // OpenJDK build-suffix special-case: NVD records for Java
            // vulns inconsistently encode the `+<build>` suffix —
            // some use `update_<n>`, some omit it entirely. Stripping
            // the suffix matches the most-common NVD shape and avoids
            // false-negative CPE-matching against scanners that key
            // on the canonical-version form. The full version stays
            // verbatim on the component's PURL.
            let cpe_version: String = if name == "openjdk" {
                version
                    .split(|c: char| !c.is_ascii_digit() && c != '.')
                    .next()
                    .unwrap_or(version)
                    .to_string()
            } else {
                version.clone()
            };
            return pairs
                .iter()
                .map(|(vendor, product)| format_cpe(vendor, product, &cpe_version))
                .collect();
        }
        _ => {
            // Unknown ecosystem — no opinion.
            return Vec::new();
        }
    }

    vendors
        .into_iter()
        .map(|vendor| format_cpe(&vendor, name, version))
        .collect()
}

/// Insert `value` (lowercased, CPE-segment-safe) into `out` unless it's
/// already present. Empty strings are dropped.
fn push_unique(out: &mut Vec<String>, value: &str) {
    let v = value.to_lowercase();
    if v.is_empty() {
        return;
    }
    if !out.iter().any(|existing| existing == &v) {
        out.push(v);
    }
}

/// Build a CPE 2.3 formatted string from (vendor, product, version).
/// The remaining seven fields are `*` (any) per spec — we don't have
/// update/edition/language/sw_edition/target_sw/target_hw/other info
/// at SBOM time.
fn format_cpe(vendor: &str, product: &str, version: &str) -> String {
    format!(
        "cpe:2.3:a:{}:{}:{}:*:*:*:*:*:*:*",
        cpe_escape(vendor),
        cpe_escape(product),
        cpe_escape(version),
    )
}

/// Escape the formatted-string special characters per CPE 2.3 §6.2.
/// The characters that require escaping inside a formatted-string
/// attribute are: `\`, `*`, `?`, `!`, `"`, `#`, `$`, `%`, `&`, `'`,
/// `(`, `)`, `+`, `,`, `/`, `:`, `;`, `<`, `=`, `>`, `@`, `[`, `]`,
/// `^`, backtick, `{`, `|`, `}`, `~`. Escape with a leading backslash.
/// Keep ASCII alphanumerics, `-`, `.`, and `_` unescaped (they're
/// safe in an attribute segment).
fn cpe_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_') {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use waybill_common::types::purl::Purl;

    fn make_component(purl_str: &str) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
        ResolvedComponent {
            build_inclusion: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    #[test]
    fn deb_produces_debian_and_product_vendor_candidates() {
        let c = make_component("pkg:deb/debian/jq@1.6-2.1+b1?distro=bookworm");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 2);
        assert!(cpes[0].starts_with("cpe:2.3:a:debian:jq:"), "{cpes:?}");
        assert!(cpes[1].starts_with("cpe:2.3:a:jq:jq:"), "{cpes:?}");
    }

    #[test]
    fn apk_produces_alpinelinux_and_product_vendor_candidates() {
        let c = make_component("pkg:apk/alpine/musl@1.2.4-r2");
        let cpes = synthesize_cpes(&c);
        assert!(cpes.iter().any(|s| s.starts_with("cpe:2.3:a:alpinelinux:musl:")));
        assert!(cpes.iter().any(|s| s.starts_with("cpe:2.3:a:musl:musl:")));
    }

    #[test]
    fn cargo_produces_product_as_vendor() {
        let c = make_component("pkg:cargo/serde@1.0.197");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 1);
        assert_eq!(
            cpes[0],
            "cpe:2.3:a:serde:serde:1.0.197:*:*:*:*:*:*:*"
        );
    }

    #[test]
    fn pypi_emits_name_and_python_prefixed_candidates() {
        let c = make_component("pkg:pypi/requests@2.31.0");
        let cpes = synthesize_cpes(&c);
        assert!(cpes.iter().any(|s| s.starts_with("cpe:2.3:a:requests:requests:")));
        assert!(cpes.iter().any(|s| s.starts_with("cpe:2.3:a:python-requests:requests:")));
    }

    #[test]
    fn npm_scoped_package_emits_scope_as_candidate() {
        let c = make_component("pkg:npm/%40angular/core@16.0.0");
        let cpes = synthesize_cpes(&c);
        assert!(
            cpes.iter().any(|s| s.contains(":angular:core:")),
            "expected angular scope as vendor, got {cpes:?}"
        );
    }

    #[test]
    fn escapes_plus_and_colon_in_version() {
        let c = make_component("pkg:deb/debian/libjq1@1.6-2.1+b1");
        let cpes = synthesize_cpes(&c);
        let primary = &cpes[0];
        // `+` must be escaped as `\+`.
        assert!(
            primary.contains("1.6-2.1\\+b1"),
            "expected escaped + in {primary}"
        );
    }

    #[test]
    fn generic_unknown_library_returns_empty() {
        // Milestone 097 FR-003: a `pkg:generic/<slug>@<version>` PURL
        // whose slug is NOT in `GENERIC_LIBRARY_CPES` (here, `weird`)
        // emits an empty Vec — silent skip, no error. Same assertion
        // as the pre-097 `unknown_ecosystem_returns_empty` test;
        // renamed because the failure mode changed from "ecosystem
        // not handled" to "library not in table".
        let c = make_component("pkg:generic/weird@1.0.0");
        let cpes = synthesize_cpes(&c);
        assert!(cpes.is_empty());
    }

    #[test]
    fn empty_version_returns_empty() {
        // Versionless components can't produce useful CPEs — the
        // version field is required by CPE 2.3 and `*` would
        // over-match.
        let mut c = make_component("pkg:cargo/serde@1.0.0");
        c.version = String::new();
        let cpes = synthesize_cpes(&c);
        assert!(cpes.is_empty(), "got {cpes:?}");
    }

    #[test]
    fn empty_name_returns_empty() {
        let mut c = make_component("pkg:cargo/serde@1.0.0");
        c.name = String::new();
        let cpes = synthesize_cpes(&c);
        assert!(cpes.is_empty(), "got {cpes:?}");
    }

    #[test]
    fn cpe_escape_preserves_safe_chars() {
        assert_eq!(cpe_escape("hello-world_1.2"), "hello-world_1.2");
        assert_eq!(cpe_escape("1.2.3"), "1.2.3");
    }

    #[test]
    fn cpe_escape_backslashes_special_chars() {
        assert_eq!(cpe_escape("1+2"), "1\\+2");
        assert_eq!(cpe_escape("a:b"), "a\\:b");
        assert_eq!(cpe_escape("a/b"), "a\\/b");
    }

    // ====================================================================
    // Milestone 097 — CPE candidate emission for binary-identified
    // `pkg:generic/<lib>@<version>` components.
    // ====================================================================

    /// T006 — Contract 1: canonical OpenSSL emission for the headline
    /// US1 case. A binary-extracted `pkg:generic/openssl@3.0.13`
    /// component receives the NVD-canonical `openssl:openssl` CPE
    /// candidate as its primary (and only) CPE.
    #[test]
    fn generic_openssl_emits_canonical_cpe() {
        let c = make_component("pkg:generic/openssl@3.0.13");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 1);
        assert_eq!(cpes[0], "cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*");
    }

    /// T007 — Contract 1 (multi-vendor): libcurl gets two NVD-cited
    /// vendor:product pairs. The first (`haxx:curl`) populates the
    /// CDX `component.cpe`; the second (`curl:curl`) flows through
    /// the existing `waybill:cpe-candidates` overflow property.
    #[test]
    fn generic_curl_emits_dual_candidates() {
        let c = make_component("pkg:generic/curl@8.4.0");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 2);
        assert_eq!(cpes[0], "cpe:2.3:a:haxx:curl:8.4.0:*:*:*:*:*:*:*");
        assert_eq!(cpes[1], "cpe:2.3:a:curl:curl:8.4.0:*:*:*:*:*:*:*");
    }

    /// T008 — Contract 4: OpenJDK build-suffix is stripped before CPE
    /// emission (`21.0.1+12` → `21.0.1`). The PURL on the component
    /// stays intact; only the CPE version segment is normalized to
    /// the most-common NVD shape.
    #[test]
    fn generic_openjdk_strips_build_suffix() {
        let c = make_component("pkg:generic/openjdk@21.0.1+12");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 1);
        assert_eq!(cpes[0], "cpe:2.3:a:oracle:openjdk:21.0.1:*:*:*:*:*:*:*");
    }

    /// T009 — FR-005 / SC-004 / Contract from US1#4: composite-evidence
    /// merge per milestone-096 Q1 produces ONE PackageDbEntry with a
    /// version-string PURL; that single component receives ONE CPE
    /// field — no duplicate emission for the symbol-fingerprint half
    /// of the evidence trail. The CPE pipeline naturally satisfies
    /// this because synthesize_cpes is per-component, not per-evidence.
    #[test]
    fn composite_evidence_emits_single_cpe() {
        let c = make_component("pkg:generic/openssl@3.0.13");
        let cpes = synthesize_cpes(&c);
        assert_eq!(
            cpes.len(),
            1,
            "milestone-096 Q1 composite-merge → ONE component → ONE CPE; got {cpes:?}"
        );
        assert_eq!(cpes[0], "cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*");
    }

    /// T011 — FR-002 maintainability: the v1 mapping table is sorted
    /// alphabetically by library_slug for diff-friendliness. New rows
    /// must keep the sort; this test fails the build when they don't.
    #[test]
    fn mappings_alphabetically_sorted() {
        let unsorted: Vec<&str> = GENERIC_LIBRARY_CPES
            .windows(2)
            .filter(|w| w[0].0 >= w[1].0)
            .map(|w| w[0].0)
            .collect();
        assert!(
            unsorted.is_empty(),
            "GENERIC_LIBRARY_CPES is not alphabetically sorted by library_slug. \
             Offending row(s): {unsorted:?}. Reorder the table."
        );
    }

    /// T012 — SC-002: every row in the mapping table emits a
    /// syntactically valid CPE 2.3 string. Loop guard against a future
    /// maintainer adding a row whose vendor or product slug breaks the
    /// `format_cpe` template. CPE 2.3 has 13 colon-separated segments
    /// (`cpe:2.3:a:vendor:product:version:update:edition:lang:sw_edition:target_sw:target_hw:other`).
    #[test]
    fn mappings_all_emit_valid_cpe23() {
        for (slug, pairs) in GENERIC_LIBRARY_CPES {
            let c = make_component(&format!("pkg:generic/{slug}@1.2.3"));
            let cpes = synthesize_cpes(&c);
            assert!(
                !cpes.is_empty(),
                "row {slug:?} produced no CPE candidates"
            );
            assert_eq!(
                cpes.len(),
                pairs.len(),
                "row {slug:?}: expected {} candidates, got {}",
                pairs.len(),
                cpes.len(),
            );
            for cpe in &cpes {
                assert!(
                    cpe.starts_with("cpe:2.3:a:"),
                    "row {slug:?} produced non-cpe23 string: {cpe}"
                );
                // CPE 2.3 formatted-string binding: 13 segments total,
                // separated by 12 unescaped colons. Escaped colons
                // (`\:`) inside an attribute don't count as separators.
                let mut separator_count = 0usize;
                let bytes = cpe.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        // Skip escaped char.
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b':' {
                        separator_count += 1;
                    }
                    i += 1;
                }
                assert_eq!(
                    separator_count, 12,
                    "row {slug:?}: CPE 2.3 must have 13 segments (12 unescaped colons), \
                     got {separator_count} in {cpe}"
                );
            }
        }
    }

    /// T013 — FR-002 / A1: the v1 mapping table must cover every
    /// curated-library slug emitted by the milestone-096 / earlier
    /// version-string scanner, EXCEPT for the documented-omission
    /// allowlist (currently just `boringssl`, whose extracted version is
    /// a git SHA and so cannot key a CPE — see the rationale on
    /// `GENERIC_LIBRARY_CPES` and `boringssl_has_no_release_version`
    /// below). Future milestones extending the scanner must either add
    /// a row here or extend the allowlist with a documented rationale.
    ///
    /// The cross-module dependency on `version_strings::CuratedLibrary`
    /// is permitted because the latter is `pub` and lives in the same
    /// crate; the test reaches it via `crate::scan_fs::binary::version_strings`.
    #[test]
    fn mappings_cover_all_curated_libraries() {
        use crate::scan_fs::binary::version_strings::CuratedLibrary;
        use strum::IntoEnumIterator;

        // Documented-omission allowlist per spec Edge Cases. An entry
        // here needs a reason that survives being checked: `boringssl`
        // is on it because its only observable version is a git SHA,
        // which `boringssl_has_no_release_version` asserts — not
        // because a namespace could not be found, which was this
        // allowlist's stated reason until #1268 and was false.
        const OMITTED: &[&str] = &["boringssl"];

        // `CuratedLibrary::iter()`, never a list of variants. This test
        // kept its own hand-written list until #1246, and by then `Php`
        // had been added to the enum, to `slug()` and to the table but
        // not to that list — so the newest variant, the one the check
        // existed for, was the one variant exempt from it. A derived
        // iterator cannot drift from the enum it is derived from, which
        // a number or a list next to the enum always can.
        for lib in CuratedLibrary::iter() {
            let slug = lib.slug();
            let in_table = GENERIC_LIBRARY_CPES.iter().any(|(s, _)| *s == slug);
            let omitted = OMITTED.contains(&slug);
            assert!(
                in_table || omitted,
                "curated library slug {slug:?} is missing from \
                 GENERIC_LIBRARY_CPES AND not on the OMITTED allowlist. \
                 Add a mapping row OR extend the allowlist with a documented rationale."
            );
        }
    }

    /// #1268: the reason `boringssl` sits on the omission allowlist,
    /// checked rather than written down. The row's rationale is that the
    /// only version waybill can observe for BoringSSL is a git revision,
    /// which no CPE version component can be built from — so if the
    /// scanner ever starts extracting a release string, the omission
    /// loses its justification and this test is what says so.
    ///
    /// Its predecessor rationale ("no NVD-tracked CPE namespace") was
    /// prose only, was false, and went unchallenged for eleven
    /// libraries. A claim that can be asserted should be.
    #[test]
    fn boringssl_has_no_release_version() {
        use crate::scan_fs::binary::version_strings::{CuratedLibrary, scan};

        // A leading NUL puts the embed at a C-string boundary, which is
        // what the scanner requires.
        let mut region = vec![0u8];
        region.extend_from_slice(b"BoringSSL aaaabbbbccccddddeeeeffff0000111122223333");
        region.push(0);

        let hits = scan(&region);
        let boringssl: Vec<_> = hits
            .iter()
            .filter(|h| h.library == CuratedLibrary::BoringSsl)
            .collect();
        assert_eq!(
            boringssl.len(),
            1,
            "expected one BoringSSL hit from the scanner, got {}",
            boringssl.len()
        );

        let version = &boringssl[0].version;
        assert!(
            version.len() == 40
                && version
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "BoringSSL's extracted version {version:?} is no longer a git SHA. \
             The `boringssl` omission in GENERIC_LIBRARY_CPES rests on it being one; \
             if the scanner now yields a release identifier, decide whether \
             `google:boringssl` (2 NVD dictionary entries, CVE-2018-12440 as of \
             2026-10-24) should get a mapping row instead (#1268)."
        );
        assert!(
            !version.contains('.'),
            "BoringSSL's extracted version {version:?} looks release-shaped; see above (#1268)"
        );
    }

    /// #1246: every `(vendor, product)` the table emits must be one NVD
    /// actually has. A pair NVD has never heard of returns zero from any
    /// CPE matcher, which reads as "no known vulnerabilities" — the
    /// failure is silent and on the safe-looking side.
    ///
    /// Hermetic: it compares the table against
    /// `NVD_DICTIONARY_OBSERVED`, which records a probe run rather than
    /// querying NVD here. So what this enforces is "no row without a
    /// recorded observation", and `scripts/probe-cpe-dictionary.sh` is
    /// how the observation is obtained.
    #[test]
    fn every_table_pair_exists_in_the_nvd_dictionary() {
        for (slug, pairs) in GENERIC_LIBRARY_CPES {
            for (vendor, product) in *pairs {
                let observed = NVD_DICTIONARY_OBSERVED
                    .iter()
                    .find(|(v, p, _)| v == vendor && p == product);
                let Some((_, _, entries)) = observed else {
                    panic!(
                        "GENERIC_LIBRARY_CPES row {slug:?} emits \
                         {vendor}:{product}, which has no entry in \
                         NVD_DICTIONARY_OBSERVED. Run \
                         `scripts/probe-cpe-dictionary.sh {vendor}:{product}` \
                         and record the result — a pair NVD does not have \
                         can only ever match zero advisories."
                    );
                };
                assert!(
                    *entries > 0,
                    "{vendor}:{product} (row {slug:?}) was probed and NVD's \
                     dictionary has no such pair, so the candidate is dead. \
                     Remove it, or correct the vendor/product."
                );
            }
        }
    }

    /// The converse: an observation left behind after its row changed
    /// would make the check above vacuous for a pair nothing emits, and
    /// would misrepresent the probe as covering the current table.
    #[test]
    fn no_dictionary_observation_is_stale() {
        for (vendor, product, _) in NVD_DICTIONARY_OBSERVED {
            let emitted = GENERIC_LIBRARY_CPES
                .iter()
                .any(|(_, pairs)| pairs.iter().any(|(v, p)| v == vendor && p == product));
            assert!(
                emitted,
                "NVD_DICTIONARY_OBSERVED carries {vendor}:{product}, which no \
                 GENERIC_LIBRARY_CPES row emits. Drop the observation with the row."
            );
        }

        let mut keys: Vec<(&str, &str)> = NVD_DICTIONARY_OBSERVED
            .iter()
            .map(|(v, p, _)| (*v, *p))
            .collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "NVD_DICTIONARY_OBSERVED has a duplicate pair");

        let sorted: Vec<(&str, &str)> = {
            let mut s: Vec<(&str, &str)> = NVD_DICTIONARY_OBSERVED
                .iter()
                .map(|(v, p, _)| (*v, *p))
                .collect();
            s.sort_unstable();
            s
        };
        let actual: Vec<(&str, &str)> = NVD_DICTIONARY_OBSERVED
            .iter()
            .map(|(v, p, _)| (*v, *p))
            .collect();
        assert_eq!(
            actual, sorted,
            "NVD_DICTIONARY_OBSERVED is not sorted by (vendor, product); \
             keep it sorted for diff-friendliness, as GENERIC_LIBRARY_CPES is."
        );
    }

    /// T015 — FR-004 / SC-003: a component with `pkg:generic/<lib>`
    /// shape and empty version (the milestone-096 symbol-fingerprint-
    /// only case + the SQLite source-id-only edge case) emits NO CPE.
    /// Wildcard-version CPEs are explicitly out of scope — they would
    /// match every CVE in the library's history and drown downstream
    /// scanners in unactionable false positives.
    ///
    /// Implementation note: this is satisfied for free by the existing
    /// `cpe.rs:25-28` empty-version fast-return, ahead of the
    /// `match ecosystem` dispatch. The test guards against regression.
    #[test]
    fn generic_symbol_fingerprint_only_emits_no_cpe() {
        let mut c = make_component("pkg:generic/openssl@dummy");
        c.version = String::new();
        let cpes = synthesize_cpes(&c);
        assert!(
            cpes.is_empty(),
            "FR-004: versionless generic component must not emit a CPE; got {cpes:?}"
        );
    }
}