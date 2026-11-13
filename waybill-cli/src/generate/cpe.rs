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
//!
//! # The curated table
//!
//! `pkg:generic/<slug>` components get their CPEs from the curated table,
//! `cpe_table.toml` (spec 1491). Each row maps a library slug, the lowercase
//! value `version_strings::CuratedLibrary::slug()`,
//! `symbol_fingerprint::SymbolFingerprintMatch::library` and the Nix closure
//! tier emit, to the NVD `(vendor, product)` pairs a matcher keys on, with the
//! dictionary evidence for each pair. The file's header states its rules;
//! the tests below enforce them.
//!
//! The table records where NVD's *dictionary* has a pair. It deliberately
//! does not record whether a pair carries advisories: dictionary presence
//! and advisory coverage are different questions, and treating a zero as an
//! answer to the second is the trap recorded in #1245. Matching identifiers
//! to vulnerabilities is the scanner's job.
//!
//! ## Why BoringSSL has no row
//!
//! BoringSSL is intentionally omitted, and the reason is **not** that
//! NVD has no namespace for it. It does: probed on 2026-10-24,
//! `cpe:2.3:a:google:boringssl` matched 2 dictionary entries, a keyword
//! search for `boringssl` returned those same 2 and nothing else, and
//! `virtualMatchString=cpe:2.3:a:google:boringssl:*:*:*:*:*:*:*:*`
//! returned one advisory, CVE-2018-12440 (positive control:
//! `openssl:openssl:3.0.11` returned 41). This comment claimed the
//! opposite until #1268.
//!
//! The reason is that BoringSSL has no release versioning. Its
//! signature is `BoringSSL <40-char-sha>`, so the version waybill
//! extracts is a git revision (`parse_git_sha` in
//! `scan_fs/binary/version_strings.rs` accepts exactly 40 lowercase hex
//! bytes and nothing else). A CPE's version component is a release
//! string that a matcher orders against advisory ranges; a git SHA
//! cannot be ordered against anything, so a `google:boringssl` CPE
//! built from one matches either nothing or arbitrarily. That is the
//! same hazard the table already refuses when it declines
//! wildcard-version CPEs, which match a library's whole history.
//!
//! **The general rule this row stands for**: a library whose only
//! observable version is not a release identifier gets no CPE,
//! whether or not a namespace exists for it. The namespace question is
//! settled by `scripts/probe-cpe-dictionary.sh`; it is necessary and
//! not sufficient. The inverse reading — "I could not find an NVD
//! namespace, so I left it out" — is the unverified assumption the
//! probe exists to prevent, and is how upstream shipped `expat:expat`
//! (anchore/syft#4788) and display-name CPEs (anchore/syft#5429).
//!
//! Components for the `boringssl` slug therefore emit a PURL with no
//! CPE field — an explicit "we don't know" per Constitution X
//! transparency. BoringSSL-rooted vulnerabilities still reach a
//! consumer through `openssl:openssl` records where the issue exists
//! upstream.
//!
//! The distinction is not carried in the SBOM, and #1268 asked whether
//! `waybill:unresolved-reason` (C151) should carry it. It cannot: C151
//! is emitted if and only if the component carries
//! `waybill:sbom-tier: "design"`, and a version-string-scanner
//! component is binary tier. Saying "this product's versioning cannot
//! key a CPE" on the wire needs a carrier that applies to a resolved
//! component, which is a separate question from this row's rationale.
//!
//! ## Names deliberately absent (milestone 1375, #1381)
//!
//! Each is in NVD's dictionary and each would be wrong. They are recorded
//! here rather than only in a spec directory because a contributor adding
//! one will be editing *this file*, and all three are attractive: the name
//! matches and the entry counts look convincing.
//!
//! - **`ninja`** — 126 entries across three vendors, and all three are other
//!   products: "Ninja Framework Ninja 7.0.0" (a Java web framework), "ITRS
//!   Group Ninja", "Visualshortcodes Ninja 1.4 for WordPress". The build
//!   tool is not in NVD at all.
//!
//! - **`cmake`** — one vendor, `cmake_project`, whose title is "cmake
//!   Project cmake 0.0.1 **for Node.js**": an npm package.
//!   `cpe:2.3:a:kitware:cmake` returns **0**. This one matters most, because
//!   a "single vendor is safe" rule would have accepted it.
//!
//! - **`libxcrypt`** — a single entry under `elektrobit`, an automotive
//!   software vendor. The version in the title is a real upstream release,
//!   so the entry may be upstream coverage filed under a downstream vendor,
//!   or a vendored copy. The dictionary cannot say which, and a wrong vendor
//!   matches nothing. Rejected in the conservative direction: a missing CPE
//!   understates, a wrong one asserts.
//!
//! Two more, rejected by `specs/nix-curated-cpe-gaps/` (2026-11-10), where
//! sbomnix 1.9.0 emits both and grype reports 13 and 10 findings from them.
//! In the measured closures each is a Haskell package, fetched from
//! `mirror://hackage/`, and the closure tier emits it as `pkg:generic` when
//! the pinned Hackage package set is unavailable:
//!
//! - **`warp`** — Haskell's WAI web server. NVD's only `warp` is
//!   `cloudflare:warp`, "Cloudflare WARP for Windows", 80 entries.
//!
//! - **`cmark-gfm`** — the Hackage binding, version `0.2.6`. NVD's
//!   `github:cmark-gfm` is GitHub's C library, versioned `0.29.0.gfm.N`, and
//!   `0.2.6` orders below every one of them. nixpkgs' C `cmark-gfm` has the
//!   same pname, so a row cannot tell the two apart by name.
//!
//! `m1375_the_rejected_names_emit_no_cpe` and
//! `nix_gap_bindings_and_prefixed_names_get_no_cpe` enforce these. The tests say what;
//! this comment says why; neither substitutes for the other.

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::cpe::Cpe;


/// Build the set of CPE 2.3 candidate strings for a resolved component.
/// Returns empty when the component is in an ecosystem the synthesizer
/// has no opinion on (generic/unknown PURLs).
pub fn synthesize_cpes(component: &ResolvedComponent) -> Vec<Cpe> {
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
        // There is deliberately **no `hackage` arm** (milestone 1373, #1375).
        //
        // It looks like an omission: 163 `pkg:hackage` components on the
        // pinned closure reach this dispatch and get nothing. It is a
        // measured decision, and the measurement is committed under
        // `specs/1373-hackage-cpe-decision/measurements/`.
        //
        // NVD's CPE dictionary was probed for every one of those names, plus
        // the capitalised ones re-probed lowercased — 171 pairs, **zero
        // unanswered**, controls at `openssl:openssl` = 385 and a fixture
        // negative = 0. **One pair has an entry**: `haskell:aeson`, 95. Case
        // makes no difference.
        //
        // The ratio is the weak argument. The strong one is which names are
        // missing: `base`, `Cabal` and `process` carry live HSEC advisories
        // *and* have no dictionary entry at all. A blanket arm would emit
        // `cpe:2.3:a:haskell:base:<v>:*:*:*:*:*:*:*` for precisely the
        // packages in this closure that have known vulnerabilities, and it
        // would match nothing — a silent zero, indistinguishable from a clean
        // component. That is the failure `scripts/probe-cpe-dictionary.sh`
        // exists to prevent, and whose header records upstream shipping it
        // twice.
        //
        // These components are not unreachable. OSV indexes Hackage and
        // answers the PURL this document already carries: six of the 163
        // return advisories, with the affected-version range applied. C209
        // counts them `addressable_without_cpe` rather than as a gap.
        //
        // A single-entry `aeson` table was considered and rejected: one row,
        // for one package, that OSV already reaches, against a standing
        // obligation to re-probe as NVD changes.
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
                return Cpe::parse(&format!(
                    "cpe:2.3:a:golang:go:{bare_version}:*:*:*:*:*:*:*"
                ))
                .into_iter()
                .collect();
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
            let Some(pairs) = crate::generate::cpe_table::built_in().pairs(name) else {
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
                .filter_map(|(vendor, product)| format_cpe(vendor, product, &cpe_version))
                .collect();
        }
        _ => {
            // Unknown ecosystem — no opinion.
            return Vec::new();
        }
    }

    vendors
        .into_iter()
        .filter_map(|vendor| format_cpe(&vendor, name, version))
        .collect()
}

/// The CPEs after the primary that each format also hands a CPE matcher
/// (#1430).
///
/// CycloneDX `component.cpe` is single-valued, so a curated row's second
/// vendor used to reach CycloneDX only in `waybill:cpe-candidates`, which no
/// scanner reads, and SPDX 2.3 only as that annotation. grype 0.120.0 reads
/// `component.cpe` plus every `syft:cpe23` property in CycloneDX, and every
/// `cpe23Type` external reference in SPDX 2.3; it does not read
/// `evidence.identity`. Measured: all 14 grype findings on `unzip 6.0` are
/// filed under `unzip_project`, then the row's second vendor, and none
/// surfaced. #1490 has since made it the first.
/// See `specs/1430-secondary-cpe-carrier/`.
///
/// **Only `pkg:generic` components.** Their CPEs come solely from
/// the curated table (`cpe_table.toml`), whose every pair was probed against NVD's
/// dictionary. Every other ecosystem's extra candidates are vendor guesses
/// (`debian:jq`, `python-<name>`), and grype's apk matcher matches every CPE
/// it is given, so exposing those is a separate decision with its own
/// measurement. A candidate without a release version is never returned: a
/// matcher would read a wildcard version as every version.
pub fn secondary_cpes(component: &ResolvedComponent) -> Vec<&str> {
    if component.purl.ecosystem() != "generic" {
        return Vec::new();
    }
    component
        .cpes
        .iter()
        .skip(1)
        .map(Cpe::as_str)
        .filter(|cpe| crate::generate::spdx::v3_external_ids::is_fully_resolved_cpe23(cpe))
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
///
/// `None` only when the result does not parse as a [`Cpe`], which an empty
/// vendor or product would cause.
pub(crate) fn format_cpe(vendor: &str, product: &str, version: &str) -> Option<Cpe> {
    Cpe::parse(&format!(
        "cpe:2.3:a:{}:{}:{}:*:*:*:*:*:*:*",
        cpe_escape(vendor),
        cpe_escape(product),
        cpe_escape(version),
    ))
    .ok()
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
            artifact_presence: None,
            scope_evidence: None,
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

    fn table() -> &'static crate::generate::cpe_table::CpeTable {
        crate::generate::cpe_table::built_in().table()
    }

    /// `(slug, pairs)` for every built-in row.
    fn rows() -> impl Iterator<Item = (&'static str, &'static [crate::generate::cpe_table::CpePair])> {
        table().row.iter().map(|r| {
            (r.key.strip_prefix("pkg:generic/").unwrap(), r.pair.as_slice())
        })
    }

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn read_evidence(path: &str) -> String {
        std::fs::read_to_string(repo_root().join(path))
            .unwrap_or_else(|e| panic!("evidence {path}: {e}"))
    }

    /// #1430 — a curated row's second vendor is handed to the matcher; the
    /// primary is not repeated.
    #[test]
    fn i1430_a_curated_rows_second_vendor_is_a_secondary_cpe() {
        let mut c = make_component("pkg:generic/unzip@9.9.9");
        c.cpes = synthesize_cpes(&c);
        // CONTROL: the row really has two vendors, or this passes vacuously.
        assert_eq!(c.cpes.len(), 2, "{:?}", c.cpes);
        assert_eq!(
            secondary_cpes(&c),
            vec!["cpe:2.3:a:info-zip:unzip:9.9.9:*:*:*:*:*:*:*"]
        );

        let mut file = make_component("pkg:generic/file@9.9");
        file.cpes = synthesize_cpes(&file);
        assert_eq!(
            secondary_cpes(&file),
            vec![
                "cpe:2.3:a:christos_zoulas:file:9.9:*:*:*:*:*:*:*",
                "cpe:2.3:a:gentoo:file:9.9:*:*:*:*:*:*:*",
            ],
            "every vendor after the first, in table order"
        );
    }

    /// #1430 — a single-vendor row has nothing to add.
    #[test]
    fn i1430_a_single_vendor_row_has_no_secondary_cpe() {
        let mut c = make_component("pkg:generic/zlib@9.9.9");
        c.cpes = synthesize_cpes(&c);
        assert_eq!(c.cpes.len(), 1, "CONTROL: {:?}", c.cpes);
        assert!(secondary_cpes(&c).is_empty());
    }

    /// #1430 — heuristic candidates in other ecosystems stay out of the
    /// matcher's reach, though they keep their `waybill:cpe-candidates` entry.
    #[test]
    fn i1430_heuristic_candidates_are_not_secondary_cpes() {
        for purl in [
            "pkg:deb/debian/jq@1.6-2.1?distro=bookworm",
            "pkg:apk/alpine/musl@1.2.4-r2",
            "pkg:pypi/requests@2.31.0",
        ] {
            let mut c = make_component(purl);
            c.cpes = synthesize_cpes(&c);
            assert!(c.cpes.len() > 1, "CONTROL {purl}: {:?}", c.cpes);
            assert!(secondary_cpes(&c).is_empty(), "{purl}: {:?}", secondary_cpes(&c));
        }
    }

    /// #1430 — a wildcard version reads as "every version" to a matcher.
    #[test]
    fn i1430_a_candidate_without_a_release_version_is_not_handed_on() {
        let mut c = make_component("pkg:generic/waybill-fixture-lib@9.9.9");
        c.cpes = vec![
            Cpe::parse("cpe:2.3:a:waybill-fixture-a:waybill-fixture-lib:9.9.9:*:*:*:*:*:*:*").unwrap(),
            Cpe::parse("cpe:2.3:a:waybill-fixture-b:waybill-fixture-lib:*:*:*:*:*:*:*:*").unwrap(),
            Cpe::parse("cpe:2.3:a:waybill-fixture-c:waybill-fixture-lib:9.9.9:*:*:*:*:*:*:*").unwrap(),
        ];
        assert_eq!(
            secondary_cpes(&c),
            vec!["cpe:2.3:a:waybill-fixture-c:waybill-fixture-lib:9.9.9:*:*:*:*:*:*:*"]
        );
    }

    #[test]
    fn deb_produces_debian_and_product_vendor_candidates() {
        let c = make_component("pkg:deb/debian/jq@1.6-2.1+b1?distro=bookworm");
        let cpes = synthesize_cpes(&c);
        assert_eq!(cpes.len(), 2);
        assert!(cpes[0].as_str().starts_with("cpe:2.3:a:debian:jq:"), "{cpes:?}");
        assert!(cpes[1].as_str().starts_with("cpe:2.3:a:jq:jq:"), "{cpes:?}");
    }

    #[test]
    fn apk_produces_alpinelinux_and_product_vendor_candidates() {
        let c = make_component("pkg:apk/alpine/musl@1.2.4-r2");
        let cpes = synthesize_cpes(&c);
        assert!(cpes.iter().any(|s| s.as_str().starts_with("cpe:2.3:a:alpinelinux:musl:")));
        assert!(cpes.iter().any(|s| s.as_str().starts_with("cpe:2.3:a:musl:musl:")));
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
        assert!(cpes.iter().any(|s| s.as_str().starts_with("cpe:2.3:a:requests:requests:")));
        assert!(cpes.iter().any(|s| s.as_str().starts_with("cpe:2.3:a:python-requests:requests:")));
    }

    #[test]
    fn npm_scoped_package_emits_scope_as_candidate() {
        let c = make_component("pkg:npm/%40angular/core@16.0.0");
        let cpes = synthesize_cpes(&c);
        assert!(
            cpes.iter().any(|s| s.as_str().contains(":angular:core:")),
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
            primary.as_str().contains("1.6-2.1\\+b1"),
            "expected escaped + in {primary}"
        );
    }

    #[test]
    fn generic_unknown_library_returns_empty() {
        // Milestone 097 FR-003: a `pkg:generic/<slug>@<version>` PURL
        // whose slug is NOT in the curated table (here, `weird`)
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
        let rows = &table().row;
        // CONTROL: the table is there, or an empty one passes vacuously.
        assert!(rows.len() > 1, "the curated table has {} rows", rows.len());
        let unsorted: Vec<&str> = rows
            .windows(2)
            .filter(|w| w[0].key >= w[1].key)
            .map(|w| w[0].key.as_str())
            .collect();
        assert!(
            unsorted.is_empty(),
            "cpe_table.toml is not sorted by key. \
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
        for (slug, pairs) in rows() {
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
                    cpe.as_str().starts_with("cpe:2.3:a:"),
                    "row {slug:?} produced non-cpe23 string: {cpe}"
                );
                // CPE 2.3 formatted-string binding: 13 segments total,
                // separated by 12 unescaped colons. Escaped colons
                // (`\:`) inside an attribute don't count as separators.
                let mut separator_count = 0usize;
                let bytes = cpe.as_str().as_bytes();
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
    /// the curated table and `boringssl_has_no_release_version`
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
            let in_table = rows().any(|(s, _)| s == slug);
            let omitted = OMITTED.contains(&slug);
            assert!(
                in_table || omitted,
                "curated library slug {slug:?} is missing from \
                 cpe_table.toml AND not on the OMITTED allowlist. \
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
             The `boringssl` omission in cpe_table.toml rests on it being one; \
             if the scanner now yields a release identifier, decide whether \
             `google:boringssl` (2 NVD dictionary entries, CVE-2018-12440 as of \
             2026-10-24) should get a mapping row instead (#1268)."
        );
        assert!(
            !version.contains('.'),
            "BoringSSL's extracted version {version:?} looks release-shaped; see above (#1268)"
        );
    }

    /// T008 (FR-006) — the vendor is NVD's, not the component name.
    ///
    /// These are the rows a future edit is most likely to "correct" to
    /// `(name, name)`. Each such correction would match nothing, silently.
    #[test]
    fn m1375_the_vendor_is_nvds_not_the_component_name() {
        for (slug, want_vendor) in [
            ("bash", "gnu"),
            // NVD files expat's CVEs under `libexpat_project:libexpat`;
            // `libexpat:expat` exists in the dictionary with 0 (2026-11-12).
            ("expat", "libexpat_project"),
            ("libxml2", "xmlsoft"),
            ("brotli", "google"),
            ("gawk", "fossies"),
            ("compiler-rt", "llvm"),
        ] {
            let row = rows()
                .find(|(s, _)| *s == slug)
                .unwrap_or_else(|| panic!("no table row for {slug}"));
            let vendors: Vec<&str> = row.1.iter().map(|p| p.vendor.as_str()).collect();
            assert!(
                vendors.contains(&want_vendor),
                "{slug} must emit vendor {want_vendor}, got {vendors:?} — \
                 NVD does not file it under its own name"
            );
            assert!(
                !vendors.contains(&slug),
                "{slug} emits vendor {slug}, which NVD does not have. A pair \
                 NVD lacks matches nothing, silently."
            );
        }
    }

    /// T009 — no duplicate slug, so a second row cannot shadow the first.
    #[test]
    fn m1375_no_duplicate_slug() {
        let mut seen = std::collections::HashSet::new();
        for (slug, _) in rows() {
            assert!(seen.insert(slug), "duplicate slug {slug} in the table");
        }
        // CONTROL: the loop saw the whole table.
        assert_eq!(seen.len(), table().row.len());
    }

    /// T009a (FR-002, SC-002) — every row's **product** has a committed title
    /// query behind it.
    ///
    /// The dictionary guard proves a pair is *in* NVD; it cannot prove the
    /// pair is the right *product* — `ninja` would satisfy it three times
    /// over. That judgement is in `research.md` R4; this asserts its
    /// traceability, which is the machine-checkable half.
    ///
    /// **Keyed on the product, not the slug.** `node`'s slug is `node` while
    /// its product is `node.js`; a slug-keyed check reports zero entries for
    /// a correct row.
    #[test]
    fn m1375_every_product_has_a_committed_title_query() {
        // CONTROL: a known row's title file holds its query, or every check
        // below could pass over files that were never read.
        let bash = rows().find(|(s, _)| *s == "bash").unwrap().1;
        assert!(
            read_evidence(bash[0].title_evidence.as_deref().unwrap()).contains("=== bash ("),
            "bash's title evidence does not hold its query; the checks below would be vacuous"
        );

        for (slug, pairs) in rows() {
            for pair in pairs {
                let path = pair.title_evidence.as_deref().unwrap();
                assert!(
                    read_evidence(path).contains(&format!("=== {} (", pair.product)),
                    "table row {slug:?} emits product {:?}, whose title_evidence \
                     {path} has no title query for it. Run `cpe-titles.py {}` and \
                     record it — an entry count says a pair exists, not that \
                     it is the same software.",
                    pair.product,
                    pair.product,
                );
            }
        }
    }

    /// T016 (SC-003) — the three rejected names emit nothing.
    #[test]
    fn m1375_the_rejected_names_emit_no_cpe() {
        // CONTROL: the synthesizer produces candidates at all, or the
        // assertions below pass just as well when it is broken outright.
        let control = make_component("pkg:generic/bash@5.3");
        assert!(
            !synthesize_cpes(&control).is_empty(),
            "the control must produce candidates, or the rejections below are \
             vacuous"
        );

        for name in ["ninja", "cmake", "libxcrypt"] {
            let c = make_component(&format!("pkg:generic/{name}@1.0.0"));
            assert!(
                synthesize_cpes(&c).is_empty(),
                "{name} gained a CPE. See the rejections recorded above the \
                 table: NVD's entries for it are a different product, and a \
                 row here would assert software is present that is not."
            );
            assert!(
                !rows().any(|(s, _)| s == name),
                "{name} was added to the table; #1381 rejected it"
            );
        }
    }

    /// #1246: every `(vendor, product)` the table emits must be one NVD
    /// actually has. A pair NVD has never heard of returns zero from any
    /// CPE matcher, which reads as "no known vulnerabilities" — the
    /// failure is silent and on the safe-looking side.
    ///
    /// Hermetic: each pair records a probe's observation, and its evidence
    /// file is committed. What this enforces is "no pair without a recorded
    /// observation"; `scripts/probe-cpe-dictionary.sh` is how one is made.
    #[test]
    fn every_table_pair_exists_in_the_nvd_dictionary() {
        for (slug, pairs) in rows() {
            for pair in pairs {
                let entries = pair.dictionary_entries.unwrap_or_else(|| {
                    panic!(
                        "row {slug:?} emits {}:{} with no dictionary_entries. Run \
                         `scripts/probe-cpe-dictionary.sh {}:{}` and record the \
                         result — a pair NVD does not have can only ever match zero.",
                        pair.vendor, pair.product, pair.vendor, pair.product,
                    )
                });
                assert!(
                    entries > 0,
                    "{}:{} (row {slug:?}) was probed and NVD's dictionary has no \
                     such pair, so the candidate is dead. Remove it, or correct \
                     the vendor/product.",
                    pair.vendor,
                    pair.product,
                );
            }
        }
    }

    /// Spec 1491 FR-013 — every evidence file a pair names is committed.
    #[test]
    fn m1491_every_evidence_path_exists() {
        let mut checked = 0;
        for (slug, pairs) in rows() {
            for pair in pairs {
                for path in [&pair.dictionary_evidence, &pair.title_evidence] {
                    let path = path.as_deref().unwrap();
                    assert!(
                        repo_root().join(path).is_file(),
                        "row {slug:?} ({}:{}) names evidence {path}, which is not a \
                         committed file",
                        pair.vendor,
                        pair.product,
                    );
                    checked += 1;
                }
            }
        }
        // CONTROL: two paths for each of the 96 pair occurrences.
        assert_eq!(checked, 2 * 96);
    }

    /// Spec 1491 FR-013, research R1 — every recorded count is the count its
    /// dictionary evidence holds.
    ///
    /// Reads the evidence by its header (`product`, `vendor`,
    /// `dictionary-entries`) and skips `#` lines. The census's
    /// `# control+  openssl:openssl  330` line is a vulnerability count, and
    /// reading by position or pattern would take it for a second
    /// `openssl:openssl` count.
    #[test]
    fn m1491_every_count_matches_its_evidence() {
        let mut files: std::collections::HashMap<String, std::collections::HashMap<(String, String), u32>> =
            std::collections::HashMap::new();
        for (slug, pairs) in rows() {
            for pair in pairs {
                let path = pair.dictionary_evidence.clone().unwrap();
                let counts = files
                    .entry(path.clone())
                    .or_insert_with(|| dictionary_counts(&read_evidence(&path)));
                // CONTROL: the evidence parsed to rows, and to a known one.
                assert!(
                    counts.len() >= 95 && counts.contains_key(&("gnu".into(), "bash".into())),
                    "{path} parsed to {} rows without gnu:bash; the check would be vacuous",
                    counts.len(),
                );
                let want = counts.get(&(pair.vendor.clone(), pair.product.clone()));
                assert_eq!(
                    want.copied(),
                    pair.dictionary_entries,
                    "row {slug:?}: {}:{} records {:?} dictionary entries, but {path} \
                     holds {want:?}",
                    pair.vendor,
                    pair.product,
                    pair.dictionary_entries,
                );
            }
        }
    }

    /// Spec 1491 FR-013 — a pair two rows share carries the same evidence in
    /// both, so the two rows cannot disagree about one observation.
    #[test]
    fn m1491_shared_pairs_agree() {
        let mut seen: std::collections::HashMap<(&str, &str), &crate::generate::cpe_table::CpePair> =
            std::collections::HashMap::new();
        let mut shared = 0;
        for (slug, pairs) in rows() {
            for pair in pairs {
                let key = (pair.vendor.as_str(), pair.product.as_str());
                if let Some(first) = seen.insert(key, pair) {
                    shared += 1;
                    assert_eq!(
                        (&first.dictionary_entries, &first.observed, &first.dictionary_evidence),
                        (&pair.dictionary_entries, &pair.observed, &pair.dictionary_evidence),
                        "row {slug:?} records {}:{} differently from an earlier row",
                        pair.vendor,
                        pair.product,
                    );
                }
            }
        }
        // CONTROL: python:python is shared by python3 and python3-minimal.
        assert_eq!(shared, 1, "expected exactly one shared pair (python:python)");
    }

    /// Spec 1491 FR-011, research R10 — the table holds identifiers and
    /// dictionary evidence only. No field is about vulnerabilities, and no
    /// line carries an advisory identifier or a count of them.
    #[test]
    fn m1491_the_table_carries_no_vulnerability_data() {
        let text = include_str!("cpe_table.toml");
        let banned_key = regex::Regex::new(r"(?im)^\s*\{?.*\b(cves?|advisor\w*|vuln\w*)\s*=").unwrap();
        let advisory_id = regex::Regex::new(r"(?i)\b(cve|ghsa|osv)-\d").unwrap();
        let count = regex::Regex::new(r"(?i)\b\d+\s+(cves?|advisories|vulnerabilities)\b").unwrap();
        for (i, line) in text.lines().enumerate() {
            for (what, re) in
                [("a vulnerability field", &banned_key), ("an advisory identifier", &advisory_id), ("a vulnerability count", &count)]
            {
                assert!(!re.is_match(line), "cpe_table.toml line {} holds {what}: {line}", i + 1);
            }
        }
        // CONTROL: the patterns fire on what they are meant to catch.
        assert!(banned_key.is_match("  { vendor = \"v\", cves = 3 },"));
        assert!(advisory_id.is_match("# see CVE-2026-1"));
        assert!(count.is_match("# 28 CVEs under invisible-island"));
    }

    /// The census shape: `# product<TAB>vendor<TAB>dictionary-entries...`
    /// header, `#` comments skipped.
    fn dictionary_counts(text: &str) -> std::collections::HashMap<(String, String), u32> {
        let mut header: Vec<&str> = Vec::new();
        let mut out = std::collections::HashMap::new();
        for line in text.lines() {
            if let Some(h) = line.strip_prefix("# product\t") {
                header = std::iter::once("product").chain(h.split('\t')).collect();
                continue;
            }
            if line.is_empty() || line.starts_with('#') || header.is_empty() {
                continue;
            }
            let rec: std::collections::HashMap<&str, &str> =
                header.iter().copied().zip(line.split('\t')).collect();
            if let (Some(p), Some(v), Some(n)) =
                (rec.get("product"), rec.get("vendor"), rec.get("dictionary-entries"))
            {
                if let Ok(n) = n.parse() {
                    out.insert(((*v).to_string(), (*p).to_string()), n);
                }
            }
        }
        out
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

    /// #1395 — the CPython interpreter, as nixpkgs names it, is matchable.
    ///
    /// nixpkgs' `python3` / `python313` / `python312` / `python314` all
    /// build a derivation whose pname is `python3`, and `python3Minimal`'s is
    /// `python3-minimal`. None states a vendor, so none carries a declared
    /// CPE. NVD files every CPython record under `python:python`, with the
    /// release in the version field (`3.13.13`, not `3.13`).
    #[test]
    fn i1395_the_cpython_interpreter_maps_to_python_python() {
        for (name, version, want) in [
            ("python3", "3.13.13", "cpe:2.3:a:python:python:3.13.13:*:*:*:*:*:*:*"),
            ("python3-minimal", "3.13.13", "cpe:2.3:a:python:python:3.13.13:*:*:*:*:*:*:*"),
        ] {
            let c = make_component(&format!("pkg:generic/{name}@{version}"));
            assert_eq!(synthesize_cpes(&c), vec![Cpe::parse(want).unwrap()], "{name}");
        }
    }

    /// #1395 — only the interpreter. A nixpkgs Python *package* is named
    /// `python3.13-<pkg>`, its hooks `python-<hook>`, and an interpreter
    /// environment `python3-<v>-env`; none of them is CPython, and a
    /// `python:python` CPE on one would attach every interpreter CVE to it.
    /// Neither is `pypy3.11`, nor Microsoft's VS Code extension, which NVD
    /// also files under the product `python`.
    #[test]
    fn i1395_names_that_are_not_the_interpreter_get_no_cpe() {
        // CONTROL: the interpreter row exists, or every assertion below is
        // vacuous.
        let control = make_component("pkg:generic/python3@3.13.13");
        assert!(
            !synthesize_cpes(&control).is_empty(),
            "the control must produce a candidate, or the rejections below are vacuous"
        );

        for name in [
            "python",
            "python3.13",
            "python313",
            "python3.13-waybill-fixture-pkg",
            "python3-waybill-fixture-pkg",
            "python-setup-hook.sh",
            "python-catch-conflicts-hook",
            "pypy3.11",
        ] {
            let c = make_component(&format!("pkg:generic/{name}@3.13.13"));
            assert!(
                synthesize_cpes(&c).is_empty(),
                "{name} gained a CPE; only the interpreter is CPython (#1395)"
            );
        }
    }

    /// The C libraries a Nix closure carries that nixpkgs declares no vendor
    /// for, mapped to the pair NVD files them under. Versions are nixpkgs' at
    /// `a799d3e3`, each of which is itself a dictionary entry except
    /// libssh 0.12.0 and libass 0.17.4 (newer than NVD's latest entry, same
    /// dotted shape). See `specs/nix-curated-cpe-gaps/`.
    #[test]
    fn nix_gap_closure_c_libraries_map_to_nvds_pair() {
        for (name, version, want) in [
            ("cjson", "1.7.19", &[
                "cpe:2.3:a:davegamble:cjson:1.7.19:*:*:*:*:*:*:*",
                "cpe:2.3:a:cjson_project:cjson:1.7.19:*:*:*:*:*:*:*",
            ][..]),
            ("fontforge", "20251009", &["cpe:2.3:a:fontforge:fontforge:20251009:*:*:*:*:*:*:*"][..]),
            ("giflib", "5.2.2", &["cpe:2.3:a:giflib_project:giflib:5.2.2:*:*:*:*:*:*:*"][..]),
            ("libass", "0.17.4", &["cpe:2.3:a:libass_project:libass:0.17.4:*:*:*:*:*:*:*"][..]),
            (
                "libsndfile",
                "1.2.2",
                &["cpe:2.3:a:libsndfile_project:libsndfile:1.2.2:*:*:*:*:*:*:*"][..],
            ),
            ("libssh", "0.12.0", &["cpe:2.3:a:libssh:libssh:0.12.0:*:*:*:*:*:*:*"][..]),
            ("unbound", "1.25.0", &["cpe:2.3:a:nlnetlabs:unbound:1.25.0:*:*:*:*:*:*:*"][..]),
        ] {
            let c = make_component(&format!("pkg:generic/{name}@{version}"));
            assert_eq!(synthesize_cpes(&c), want, "{name}");
        }
    }

    /// The rejections, and the names next to the rows that are not them.
    ///
    /// `warp` and `cmark-gfm` are in these closures only as Hackage packages
    /// (`mirror://hackage/warp-3.4.9.tar.gz`), and when the pinned package
    /// set is unavailable the closure tier emits them as `pkg:generic`, which
    /// is the arm this table serves. NVD's only `warp` is Cloudflare WARP; its
    /// `cmark-gfm` is GitHub's C library, versioned `0.29.0.gfm.N`, against
    /// which a binding's `0.2.6` orders below every fixed version. A row for
    /// either would assert software is present that is not, which is what
    /// sbomnix 1.9.0 does for both.
    ///
    /// The rest are the prefixed and binding names a closure carries beside
    /// the libraries: none is the library, so the rows are exact names.
    #[test]
    fn nix_gap_bindings_and_prefixed_names_get_no_cpe() {
        // CONTROL: the rows exist, or every assertion below is vacuous.
        for control in ["pkg:generic/unbound@1.25.0", "pkg:generic/libssh@0.12.0"] {
            assert!(
                !synthesize_cpes(&make_component(control)).is_empty(),
                "the control {control} must produce a candidate, or the rejections \
                 below are vacuous"
            );
        }

        for purl in [
            "pkg:generic/warp@3.4.9",
            "pkg:hackage/warp@3.4.9",
            "pkg:generic/cmark-gfm@0.2.6",
            "pkg:hackage/cmark-gfm@0.2.6",
            "pkg:generic/cmark-gfm@0.29.0.gfm.13",
            "pkg:hackage/libssh@0.12.0",
            "pkg:hackage/unbound@1.25.0",
            "pkg:generic/python3.13-unbound@1.25.0",
            "pkg:generic/python3.13-cjson@1.7.19",
            "pkg:generic/perl5.40.0-waybill-fixture-libssh@0.12.0",
            "pkg:generic/libssh-waybill-fixture@0.12.0",
            "pkg:generic/unbound-full@1.25.0",
            "pkg:generic/giflib-tools@5.2.2",
            "pkg:generic/libsndfile1@1.2.2",
            "pkg:generic/cjson-1.7.19.tar.gz@1.7.19",
            "pkg:generic/fontforge-gtk@20251009",
            "pkg:generic/libass2@0.17.4",
        ] {
            let c = make_component(purl);
            assert!(
                synthesize_cpes(&c).is_empty(),
                "{purl} gained a CPE; the rows are exact library names, and \
                 warp/cmark-gfm were rejected (specs/nix-curated-cpe-gaps/)"
            );
        }
        for name in ["warp", "cmark-gfm"] {
            assert!(
                !rows().any(|(s, _)| s == name),
                "{name} was added to the table; specs/nix-curated-cpe-gaps/ rejected it"
            );
        }
    }

    /// Milestone 1373 (#1375) — `pkg:hackage` gets nothing, on purpose.
    ///
    /// Enforced rather than described. NVD's dictionary has 1 of the 163
    /// Haskell names in the measured closure, and the three with live HSEC
    /// advisories — `base`, `Cabal`, `process` — have no entry at all, so an
    /// arm here would emit CPEs matching nothing for exactly the packages
    /// that matter. A future arm cannot be added without confronting this.
    #[test]
    fn m1373_t023_hackage_components_get_no_cpe_candidates() {
        // CONTROL: the synthesizer produces candidates at all. Without this
        // the assertions below pass just as well when `synthesize_cpes` is
        // broken outright, which is the shape of an empty-result test that
        // proves nothing.
        let control = make_component("pkg:pypi/urllib3@2.2.1");
        assert!(
            !synthesize_cpes(&control).is_empty(),
            "the control must produce candidates, or the hackage assertions \
             below are vacuous"
        );

        for purl in ["pkg:hackage/aeson@2.2.4.1", "pkg:hackage/base@4.19.2.0"] {
            let c = make_component(purl);
            assert!(
                synthesize_cpes(&c).is_empty(),
                "a `hackage` arm was added without revisiting #1375: {purl}"
            );
        }
    }
}
