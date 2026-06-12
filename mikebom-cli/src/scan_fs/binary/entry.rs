//! BinaryScan + binary-scan-result-to-PackageDbEntry conversion.
//!
//! Owns the intermediate `BinaryScan` type that the per-file scanner
//! in `scan.rs` produces, plus the three conversion functions that
//! turn scan results into `PackageDbEntry` rows: `version_match_to_entry`
//! (curated version strings), `make_file_level_component` (the binary
//! itself), and `note_package_to_entry` (ELF .note.package parsing).

use std::path::Path;

use mikebom_common::types::hash::ContentHash;
use mikebom_common::types::purl::Purl;
use sha2::{Digest, Sha256};

use super::cargo_auditable;
use super::elf;
use super::fingerprints::matcher::MatchResult;
use super::macho;
use super::packer;
use super::symbol_fingerprint;
use super::version_strings;
use super::super::package_db::{rpm_vendor_from_id, PackageDbEntry};

/// Convert a symbol-fingerprint match into a `PackageDbEntry`. Milestone
/// 096 FR-004 / US3. PURL has no `@<version>` segment — symbol presence
/// alone can't pin a release. Confidence is intentionally lower than
/// `embedded-version-string` (0.4 vs 0.6 conceptually; both map to the
/// `heuristic` value in `mikebom:confidence` since the CDX evidence
/// pipeline currently maps ALL package-db-derived entries to
/// `manifest-analysis` / 0.85 — see milestone-097 follow-up for a real
/// confidence-tier split). The `mikebom:fingerprint-symbols-matched`
/// annotation carries the X/Y ratio for transparency per Constitution X.
pub(super) fn symbol_match_to_entry(
    m: &symbol_fingerprint::SymbolFingerprintMatch,
    path: &Path,
) -> Option<PackageDbEntry> {
    // Milestone 108 — prefer the match's explicit `target_purl` (set
    // by `scan_with_corpus` from the corpus record). The bundled-corpus
    // path emits the same `pkg:generic/<library>` shape as pre-108
    // by construction (see `symbol_fingerprint::bundled_records`).
    let purl = mikebom_common::types::purl::Purl::new(&m.target_purl).ok()?;
    // Milestone 109 — when attribution rewrote the PURL to a source-
    // tier value carrying a version (e.g. `pkg:github/madler/zlib@v1.3.1`),
    // adopt that version into the PackageDbEntry's `version` field so
    // the downstream deduplicator (`resolve::deduplicator::deduplicate`)
    // keys this binary-tier component with the SAME `(ecosystem, name,
    // version, parent_purl)` tuple as the source-tier component the
    // cmake reader emitted. Without this, the two would land in
    // separate dedup buckets and the SBOM would carry two zlib
    // components — the SC-001 single-component contract requires the
    // merge.
    let version = purl.version().unwrap_or("").to_string();
    let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra.insert(
        "mikebom:fingerprint-symbols-matched".to_string(),
        serde_json::Value::String(format!("{}/{}", m.matched_count, m.total_count)),
    );
    // Milestone 108 FR-005: stamp the corpus provenance when present.
    // `None` here means the scan was bundled-only and non-opt-in, so
    // no annotation is emitted (preserves SC-003 byte-identity).
    //
    // Milestone 110 FR-017 (US3 regression-gate): when corpus-sha is
    // emitted (i.e., operator is opted in to fingerprints), ALSO emit
    // the numeric fused-confidence value as `mikebom:fingerprint-
    // confidence`. Distinct from the existing `mikebom:confidence`
    // enum-string carrier (C16, value=`"heuristic"`) so no collision.
    // v1 records all map to the 0.70 baseline per the design doc §7
    // "threshold-met exported symbols" entry + the 2026-06-03
    // /speckit-clarify Q3 mapping. v2 records (Phase 4) will compute
    // their own values via the fusion rule and stamp that instead.
    if let Some(ref sha) = m.corpus_sha_annotation {
        extra.insert(
            "mikebom:fingerprint-corpus-sha".to_string(),
            serde_json::Value::String(sha.clone()),
        );
        extra.insert(
            "mikebom:fingerprint-confidence".to_string(),
            serde_json::Value::String("0.70".to_string()),
        );
    }
    // Milestone 108 FR-013: when the same target binary triggered
    // matches against multiple corpus records (variant collision —
    // e.g., LibreSSL + OpenSSL share `SSL_*` symbols), surface the
    // sibling matches so consumers can disambiguate.
    if !m.also_detected_via.is_empty() {
        extra.insert(
            "mikebom:also-detected-via".to_string(),
            serde_json::json!(m.also_detected_via),
        );
    }
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: m.library.clone(),
        version,
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: vec![],
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: Some("symbol-fingerprint".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: Some("heuristic".to_string()),
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: extra,
        binary_role: None,
    })
}

/// Convert a v2 matcher `MatchResult` into a `PackageDbEntry` (milestone
/// 110, Phase 4 Slice B-2).
///
/// Mirrors `symbol_match_to_entry` but uses the v2 `MatchResult` shape:
/// - PURL comes directly from the matched record (carries the version
///   segment when available, unlike v1's symbol-only `pkg:generic/<lib>`).
/// - `mikebom:fingerprint-confidence` carries the numeric fused score
///   from the matcher's "max + bump" fusion algorithm (vs v1's hardcoded
///   `"0.70"` baseline) — formatted "X.XX" per FR-017.
/// - `mikebom:fingerprint-symbols-matched` records the indicator count
///   (e.g., `"2/3"` when 2 of 3 indicator kinds in the record matched).
///   Repurposed from the v1 symbol-count carrier; v2 records carry per-
///   indicator-KIND counts rather than per-symbol counts.
/// - No `mikebom:fingerprint-corpus-sha` annotation on the v2 path —
///   the C58 carrier is v1-specific (a 12-hex git SHA from the
///   milestone-108 sibling repo). A v2-equivalent annotation
///   (`mikebom:fingerprint-source-id`) ships in a follow-on slice with
///   its own C-row.
#[allow(dead_code)] // Slice B-2 final task wires this in the scan loop.
pub(super) fn v2_match_to_entry(m: &MatchResult, path: &Path) -> PackageDbEntry {
    let purl = m.purl.clone();
    let version = purl.version().unwrap_or("").to_string();
    let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    // Numeric fused-confidence per FR-017 — formatted "X.XX" so the
    // annotation surface matches C59's documented value space (the
    // existing v1-path "0.70" string also fits this format).
    let score_str = format!("{:.2}", m.confidence_score.into_inner());
    extra.insert(
        "mikebom:fingerprint-confidence".to_string(),
        serde_json::Value::String(score_str),
    );
    // Indicator-kind count carrier — repurposed from v1's per-symbol
    // count. v2 records carry typed indicator kinds (exported_symbols /
    // version_string / build_id / macho_uuid / pe_pdb / abi_marker);
    // the count tells operators which kinds the matcher fused.
    extra.insert(
        "mikebom:fingerprint-symbols-matched".to_string(),
        serde_json::Value::String(format!(
            "{}/{}",
            m.indicators_matched.len(),
            m.indicators_matched.len()
        )),
    );
    PackageDbEntry {
        build_inclusion: None,
        purl,
        name: derive_name_from_purl(&m.purl),
        version,
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: vec![],
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: Some("symbol-fingerprint".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: Some("heuristic".to_string()),
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: extra,
        binary_role: None,
    }
}

/// Derive the human-readable `name` field from a PURL. PURL spec parses
/// `pkg:<type>/<namespace>/<name>@<version>` — we want the bare `<name>`.
#[allow(dead_code)]
fn derive_name_from_purl(purl: &Purl) -> String {
    // The Purl newtype exposes `name()` for this purpose (cf.
    // mikebom-common/src/types/purl.rs).
    purl.name().to_string()
}

/// Convert a curated-scanner match into a `PackageDbEntry`.
pub(super) fn version_match_to_entry(
    m: &version_strings::EmbeddedVersionMatch,
    path: &Path,
) -> Option<PackageDbEntry> {
    let purl_str = format!(
        "pkg:generic/{}@{}",
        mikebom_common::types::purl::encode_purl_segment(m.library.slug()),
        mikebom_common::types::purl::encode_purl_segment(&m.version),
    );
    let purl = Purl::new(&purl_str).ok()?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: m.library.slug().to_string(),
        version: m.version.clone(),
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: vec![],
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: Some("embedded-version-string".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: Some("heuristic".to_string()),
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: Default::default(),
        binary_role: None,
    })
}

/// Convert a parsed cargo-auditable manifest into per-crate
/// `PackageDbEntry` rows (milestone 029). Each manifest entry becomes
/// a `pkg:cargo/<name>@<version>` component with
/// `evidence-kind = "cargo-auditable"`, `confidence = "high"`, and
/// `parent_purl` cross-linking back to the file-level binary
/// component. Index-based `dependencies` resolve to PURL-keyed
/// `depends` edges.
///
/// PURL qualifiers per source:
/// * `registry` → no qualifier (crates.io is the implicit default).
/// * `git` / `local` / `path` / `unknown` → `?source=<source>`
///   marker so consumers can filter non-registry crates.
///
/// Output is deterministically ordered by `(name, version, source)`
/// triple — matches the cargo lockfile reader's contract so the bytes
/// are stable across scans.
pub(super) fn cargo_auditable_packages_to_entries(
    manifest: &cargo_auditable::CargoAuditableManifest,
    file_level_purl: &Purl,
    path: &Path,
) -> Vec<PackageDbEntry> {
    use mikebom_common::types::purl::encode_purl_segment;

    // First pass: build parallel `Vec<Option<Purl>>` and
    // `Vec<Option<String>>` (string form, used for the
    // `Vec<String>`-typed `depends` edges and `Option<String>`-typed
    // `parent_purl` field on PackageDbEntry).
    let purls: Vec<Option<Purl>> = manifest
        .packages
        .iter()
        .map(|p| {
            let qualifier = match p.source.as_str() {
                "registry" => String::new(),
                other => format!("?source={}", encode_purl_segment(other)),
            };
            let purl_str = format!(
                "pkg:cargo/{}@{}{}",
                encode_purl_segment(&p.name),
                encode_purl_segment(&p.version),
                qualifier,
            );
            Purl::new(&purl_str).ok()
        })
        .collect();
    let purl_strs: Vec<Option<String>> = purls
        .iter()
        .map(|p| p.as_ref().map(|x| x.as_str().to_string()))
        .collect();

    // Second pass: build the entries.
    let mut entries: Vec<PackageDbEntry> = manifest
        .packages
        .iter()
        .enumerate()
        .filter_map(|(i, pkg)| {
            let purl = purls[i].clone()?;
            let depends: Vec<String> = {
                let mut d: Vec<String> = pkg
                    .dependencies
                    .iter()
                    .filter_map(|&idx| purl_strs.get(idx).and_then(|p| p.clone()))
                    .collect();
                d.sort();
                d
            };
            let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            // Only emit `kind` annotation when present and not the
            // implied default ("runtime"). build/dev kinds are
            // worth surfacing for downstream filtering.
            if let Some(ref k) = pkg.kind {
                if k != "runtime" {
                    extra.insert(
                        "mikebom:cargo-auditable-kind".to_string(),
                        serde_json::Value::String(k.clone()),
                    );
                }
            }
            // Source annotation only when not "registry" (the
            // crates.io default is implied by the bare PURL).
            if pkg.source != "registry" {
                extra.insert(
                    "mikebom:cargo-auditable-source".to_string(),
                    serde_json::Value::String(pkg.source.clone()),
                );
            }
            Some(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                arch: None,
                source_path: path.to_string_lossy().into_owned(),
                depends,
                maintainer: None,
                licenses: vec![],
                lifecycle_scope: None,
                requirement_range: None,
                source_type: None,
                sbom_tier: Some("analyzed".to_string()),
                shade_relocation: None,
                buildinfo_status: None,
                evidence_kind: Some("cargo-auditable".to_string()),
                binary_class: None,
                binary_stripped: None,
                linkage_kind: None,
                detected_go: None,
                confidence: Some("high".to_string()),
                binary_packed: None,
                raw_version: None,
                parent_purl: Some(file_level_purl.as_str().to_string()),
                npm_role: None,
                co_owned_by: None,
                hashes: Vec::new(),
                extra_annotations: extra,
                binary_role: None,
            })
        })
        .collect();

    // Determinism: sort by (name, version, source) triple.
    entries.sort_by(|a, b| {
        let a_src = a
            .extra_annotations
            .get("mikebom:cargo-auditable-source")
            .and_then(|v| v.as_str())
            .unwrap_or("registry");
        let b_src = b
            .extra_annotations
            .get("mikebom:cargo-auditable-source")
            .and_then(|v| v.as_str())
            .unwrap_or("registry");
        (a.name.as_str(), a.version.as_str(), a_src)
            .cmp(&(b.name.as_str(), b.version.as_str(), b_src))
    });

    entries
}

/// Cross-format scan result. Common fields populated from all three
/// formats via `object::read::File::imports()`; `note_package`,
/// `build_id`, `runpath`, and `debuglink` are ELF-specific and stay at
/// their default values for Mach-O / PE.
pub(crate) struct BinaryScan {
    pub binary_class: &'static str,
    pub imports: Vec<String>,
    pub has_dynamic: bool,
    pub stripped: bool,
    pub note_package: Option<elf::ElfNotePackage>,
    /// Lowercase-hex `NT_GNU_BUILD_ID` note from `.note.gnu.build-id`
    /// (milestone 023). Typically 40 hex chars (20-byte SHA-1).
    /// `None` for non-ELF or for ELF binaries built with
    /// `-Wl,--build-id=none`.
    pub build_id: Option<String>,
    /// `DT_RPATH` / `DT_RUNPATH` paths in declaration order, dedup'd
    /// (milestone 023). `$ORIGIN` and similar substitutions are
    /// recorded raw — expansion is runtime-context-dependent.
    pub runpath: Vec<String>,
    /// `.gnu_debuglink` reference (milestone 023): NUL-terminated
    /// filename + LE CRC32 of the referenced .debug file. Pointer
    /// only — mikebom does not chase or verify the .debug file.
    pub debuglink: Option<elf::DebuglinkEntry>,
    /// Mach-O `LC_UUID` 16-byte identity, hex-encoded lowercase
    /// (milestone 024). The macOS analog of `build_id`. `None` for
    /// non-Mach-O or for Mach-O binaries built with `ld -no_uuid`.
    /// Read from the FIRST slice on fat binaries.
    pub macho_uuid: Option<String>,
    /// Mach-O `LC_RPATH` paths in declaration order, dedup'd
    /// (milestone 024). `@executable_path`, `@loader_path`, `@rpath`
    /// recorded raw — substitution is runtime-context-dependent.
    /// Read from the FIRST slice on fat binaries.
    pub macho_rpath: Vec<String>,
    /// Mach-O minimum-OS version (milestone 024). Format
    /// `<platform>:<version>` (e.g. `"macos:14.0"`, `"ios:17.5"`).
    /// Prefers `LC_BUILD_VERSION` (newer); falls back to
    /// `LC_VERSION_MIN_MACOSX` etc. when LC_BUILD_VERSION absent.
    pub macho_min_os: Option<String>,
    /// Mach-O codesign identifier (milestone 030). From the
    /// `LC_CODE_SIGNATURE` SuperBlob's `CodeDirectory.identOffset`
    /// — typically the bundle ID (`com.apple.bash`) for app-signed
    /// binaries or the basename for ad-hoc-signed ones. `None` for
    /// non-Mach-O or unsigned binaries. Read from the FIRST slice
    /// on fat binaries.
    pub macho_codesign_identifier: Option<String>,
    /// Mach-O codesign flags (milestone 030). Decoded names from
    /// the `CodeDirectory.flags` u32 bitfield (e.g.
    /// `["hardened-runtime", "library-validation"]`); unrecognized
    /// bits surface as `unknown-0x<hex>`. Empty Vec for non-Mach-O
    /// or unsigned binaries. Alphabetically sorted.
    pub macho_codesign_flags: Vec<String>,
    /// Mach-O codesign team ID (milestone 030). 10-char Apple
    /// Team ID from `CodeDirectory.teamOffset` (CD version ≥
    /// 0x20200). `None` for non-Mach-O, unsigned, or ad-hoc-signed
    /// binaries.
    pub macho_codesign_team_id: Option<String>,
    /// PE CodeView pdb-id (milestone 028). Format
    /// `<guid-hex-lowercase>:<age>` — the symbol-server identity
    /// pair from the `IMAGE_DIRECTORY_ENTRY_DEBUG` directory's
    /// CodeView Type-2 record. `None` for non-PE binaries, stripped
    /// PEs without `IMAGE_DEBUG_DIRECTORY`, or NB10 (Type-1) PDBs.
    pub pe_pdb_id: Option<String>,
    /// PE machine type (milestone 028). Lowercase string form of
    /// `IMAGE_FILE_HEADER.Machine` (`"amd64"`, `"i386"`, `"arm64"`,
    /// `"unknown"`, etc.). `None` for non-PE binaries.
    pub pe_machine: Option<String>,
    /// PE subsystem (milestone 028). Lowercase string form of
    /// `IMAGE_OPTIONAL_HEADER.Subsystem` (`"console"`,
    /// `"windows-gui"`, `"efi-application"`, `"unknown"`, etc.).
    /// `None` for non-PE binaries.
    pub pe_subsystem: Option<String>,
    /// cargo-auditable manifest extracted from the binary's `.dep-v0`
    /// linker section (milestone 029). Cross-format: ELF `.dep-v0`,
    /// Mach-O `__DATA,.dep-v0`, PE `.dep-v0`. `None` for binaries
    /// not built with `cargo auditable build`. Read from the FIRST
    /// slice on fat Mach-O binaries.
    pub cargo_auditable: Option<cargo_auditable::CargoAuditableManifest>,
    /// Concatenated read-only string-section bytes per FR-025 /
    /// research R6. Fed to the curated version-string scanner.
    /// Capped at 16 MB per binary.
    pub string_region: Vec<u8>,
    /// UPX or similar packer signature if detected (R7). `None`
    /// means no packer recognised; the linkage list is complete.
    pub packer: Option<packer::PackerKind>,
    /// Exported dynamic-symbol names from ELF `.dynsym` — fed to the
    /// symbol-fingerprint scanner (milestone 096 FR-004). Empty for
    /// non-ELF binaries, ELF binaries with no `.dynsym`, and ELF
    /// binaries whose dynamic-symbol set is fully stripped.
    pub symbol_names: Vec<String>,
    /// ELF `.comment` section's NUL-delimited compiler stamps per
    /// milestone-098 FR-001. Within-binary deduped; per-entry length
    /// capped at 4 KiB; total length capped at 64 KiB with a final
    /// `"... (truncated)"` marker entry if the cap is hit. Empty for
    /// non-ELF binaries or ELF binaries lacking the section
    /// (`strip --remove-section=.comment` or `cc -fno-ident`).
    pub comment_stamps: Vec<String>,
    /// Full `LC_BUILD_VERSION` record per milestone-098 FR-002. `None`
    /// for non-Mach-O or for Mach-O binaries lacking the command
    /// (legacy `LC_VERSION_MIN_*`-only builds). When present, drives
    /// the `mikebom:macho-build-version` + `mikebom:macho-build-tools`
    /// annotation emission on the file-level binary component.
    pub macho_build_version: Option<macho::MachoBuildVersion>,
    /// PE linker-version `<major>.<minor>` string per milestone-098
    /// FR-003. Always emitted (`Some(...)`) for any PE binary that
    /// reaches the file-level emission path — packed/obfuscated PEs
    /// with zeroed optional-header bytes emit `"0.0"` (informative,
    /// correlates with `mikebom:binary-packed`). `None` for non-PE.
    pub pe_linker_version: Option<String>,
    /// Milestone 104 — role classification (Application /
    /// SharedLibrary / Object / Other) derived from the format
    /// header. Drives the format-native component-type field at
    /// emission time.
    pub binary_role: mikebom_common::resolution::BinaryRole,
}

pub(super) fn make_file_level_component(
    path: &Path,
    bytes: &[u8],
    scan: &BinaryScan,
    detected_go: bool,
) -> PackageDbEntry {
    let sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    };
    let hash = ContentHash::sha256(&sha256)
        .expect("Sha256 hex is always well-formed");

    // File-level binary components get a synthetic pkg:generic PURL
    // keyed on sha256 so they have a stable identity. The filename
    // is preserved via the `name` field for human readability.
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    // Filename can carry arbitrary chars; percent-encode for PURL
    // name-segment conformance.
    let encoded_filename = mikebom_common::types::purl::encode_purl_segment(&filename);
    let purl_str = format!("pkg:generic/{encoded_filename}?file-sha256={sha256}");
    let purl = Purl::new(&purl_str).unwrap_or_else(|_| {
        // Fallback: use a bare generic purl if filename has chars PURL
        // can't handle. Keyed on sha256 alone.
        Purl::new(&format!("pkg:generic/binary?file-sha256={sha256}"))
            .expect("bare pkg:generic must parse")
    });

    let linkage = if scan.has_dynamic && !scan.imports.is_empty() {
        "dynamic"
    } else if !scan.has_dynamic {
        "static"
    } else {
        "dynamic"
    }
    .to_string();

    PackageDbEntry {
        build_inclusion: None,
        purl,
        name: filename,
        version: String::new(),
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: vec![],
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: Some(scan.binary_class.to_string()),
        binary_stripped: Some(scan.stripped),
        linkage_kind: Some(linkage),
        // G1: milestone 004 US2 R8 cross-link — set when the same
        // bytes carry `runtime/debug.BuildInfo` so downstream
        // consumers can pair the file-level `pkg:generic/<name>`
        // component with its `pkg:golang/<module>@<version>`
        // siblings from `go_binary.rs`.
        detected_go: if detected_go { Some(true) } else { None },
        confidence: None,
        // Milestone 096 Clarification Q2: always-emit
        // `mikebom:binary-packed` on every file-level binary
        // component. Value is the lowercase packer name when one is
        // detected (currently `"upx"` only), `"none"` otherwise.
        // Matches the existing `mikebom:binary-stripped` always-emit
        // convention so downstream filters can use value-equality
        // without presence checks.
        binary_packed: Some(
            scan.packer
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "none".to_string()),
        ),
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: build_binary_identity_annotations(scan),
        // Milestone 104 — every file-level binary component carries
        // the role classified from the format header.
        binary_role: Some(scan.binary_role),
    }
    .with_sha256_placeholder(hash)
}

/// Merge per-format identity annotations into a single bag. ELF,
/// Mach-O, and PE fields are mutually exclusive in practice (a
/// `BinaryScan` has one binary_class), so the bags don't overlap; the
/// merge is a simple extend. Four identity-cohort helpers contribute
/// as of milestone 029 (the three binary-format identity helpers
/// plus the cross-format cargo-auditable cross-link annotation).
fn build_binary_identity_annotations(
    scan: &BinaryScan,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut bag = build_elf_identity_annotations(scan);
    bag.extend(build_macho_identity_annotations(scan));
    bag.extend(build_pe_identity_annotations(scan));
    bag.extend(build_cargo_auditable_cross_link(scan));
    bag
}

/// Milestone 029: emit `mikebom:detected-cargo-auditable = true` when
/// the binary carries a parsed cargo-auditable manifest. Cross-link
/// annotation that lets consumers find the per-crate
/// `pkg:cargo/<name>@<version>` components emitted by
/// `cargo_auditable_packages_to_entries` without scanning every
/// component. The Rust analog of `mikebom:detected-go = true` (set
/// directly on `PackageDbEntry::detected_go` for Go binaries via the
/// milestone-005 `detected_go` field — kept typed because Go support
/// predates the bag).
fn build_cargo_auditable_cross_link(
    scan: &BinaryScan,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut bag = std::collections::BTreeMap::new();
    if scan.cargo_auditable.is_some() {
        bag.insert(
            "mikebom:detected-cargo-auditable".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    bag
}

/// Milestone 023: translate the three ELF identity fields on
/// `BinaryScan` into bag entries for emission. Each annotation is
/// included only when the source field is populated, so non-ELF
/// binaries (Mach-O, PE) and ELF binaries built with
/// `-Wl,--build-id=none` (etc.) emit no empty annotations.
fn build_elf_identity_annotations(
    scan: &BinaryScan,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut bag = std::collections::BTreeMap::new();
    if let Some(ref id) = scan.build_id {
        bag.insert(
            "mikebom:elf-build-id".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if !scan.runpath.is_empty() {
        bag.insert(
            "mikebom:elf-runpath".to_string(),
            serde_json::json!(scan.runpath),
        );
    }
    if let Some(ref dl) = scan.debuglink {
        bag.insert(
            "mikebom:elf-debuglink".to_string(),
            serde_json::json!({
                "file": dl.file,
                "crc32": format!("{:08x}", dl.crc32),
            }),
        );
    }
    // Milestone 098 FR-001 — ELF `.comment` compiler stamps.
    // Omit when empty (FR-007); presence of the property signals
    // "we extracted ≥1 stamp", absence is the explicit
    // "we don't know who built it" signal.
    if !scan.comment_stamps.is_empty() {
        bag.insert(
            "mikebom:elf-compiler-stamps".to_string(),
            serde_json::json!(scan.comment_stamps),
        );
    }
    bag
}

/// Milestone 024 + 030: translate the Mach-O identity + codesign
/// fields on `BinaryScan` into bag entries. Same skip-on-empty
/// contract as the ELF helper. Each annotation key names the source
/// LC_* command as a stable cross-format identity hint:
/// - `mikebom:macho-uuid` ← LC_UUID (milestone 024)
/// - `mikebom:macho-rpath` ← LC_RPATH (024)
/// - `mikebom:macho-min-os` ← LC_BUILD_VERSION or LC_VERSION_MIN_* (024)
/// - `mikebom:macho-codesign-identifier` ← LC_CODE_SIGNATURE
///   SuperBlob CodeDirectory.identOffset (milestone 030)
/// - `mikebom:macho-codesign-flags` ← CodeDirectory.flags (030)
/// - `mikebom:macho-codesign-team-id` ← CodeDirectory.teamOffset (030)
fn build_macho_identity_annotations(
    scan: &BinaryScan,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut bag = std::collections::BTreeMap::new();
    if let Some(ref uuid) = scan.macho_uuid {
        bag.insert(
            "mikebom:macho-uuid".to_string(),
            serde_json::Value::String(uuid.clone()),
        );
    }
    if !scan.macho_rpath.is_empty() {
        bag.insert(
            "mikebom:macho-rpath".to_string(),
            serde_json::json!(scan.macho_rpath),
        );
    }
    if let Some(ref min_os) = scan.macho_min_os {
        bag.insert(
            "mikebom:macho-min-os".to_string(),
            serde_json::Value::String(min_os.clone()),
        );
    }
    if let Some(ref id) = scan.macho_codesign_identifier {
        bag.insert(
            "mikebom:macho-codesign-identifier".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if !scan.macho_codesign_flags.is_empty() {
        bag.insert(
            "mikebom:macho-codesign-flags".to_string(),
            serde_json::json!(scan.macho_codesign_flags),
        );
    }
    if let Some(ref team) = scan.macho_codesign_team_id {
        bag.insert(
            "mikebom:macho-codesign-team-id".to_string(),
            serde_json::Value::String(team.clone()),
        );
    }
    // Milestone 098 FR-002 — LC_BUILD_VERSION full record. Emits two
    // properties when the load command is present:
    //   - `mikebom:macho-build-version`: structured object with
    //     platform/min_os/sdk
    //   - `mikebom:macho-build-tools`: array of {tool, version} pairs
    //     (only when the tools list is non-empty)
    // Omitted entirely (FR-007) when LC_BUILD_VERSION is absent — the
    // milestone-024 `mikebom:macho-min-os` flat property continues to
    // emit independently for legacy LC_VERSION_MIN_* binaries.
    if let Some(ref bv) = scan.macho_build_version {
        bag.insert(
            "mikebom:macho-build-version".to_string(),
            serde_json::json!({
                "platform": bv.platform,
                "min_os": bv.min_os,
                "sdk": bv.sdk,
            }),
        );
        if !bv.tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = bv
                .tools
                .iter()
                .map(|(tool, version)| serde_json::json!({"tool": tool, "version": version}))
                .collect();
            bag.insert(
                "mikebom:macho-build-tools".to_string(),
                serde_json::Value::Array(tools_json),
            );
        }
    }
    bag
}

/// Milestone 028: translate the three PE identity fields on
/// `BinaryScan` into bag entries. Same skip-on-empty contract as the
/// ELF + Mach-O helpers. Bag keys carry the source IMAGE_* field for
/// cross-format identity:
/// - `mikebom:pe-pdb-id`    ← IMAGE_DEBUG_DIRECTORY CodeView record
/// - `mikebom:pe-machine`   ← IMAGE_FILE_HEADER.Machine
/// - `mikebom:pe-subsystem` ← IMAGE_OPTIONAL_HEADER.Subsystem
fn build_pe_identity_annotations(
    scan: &BinaryScan,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut bag = std::collections::BTreeMap::new();
    if let Some(ref id) = scan.pe_pdb_id {
        bag.insert(
            "mikebom:pe-pdb-id".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if let Some(ref machine) = scan.pe_machine {
        bag.insert(
            "mikebom:pe-machine".to_string(),
            serde_json::Value::String(machine.clone()),
        );
    }
    if let Some(ref subsystem) = scan.pe_subsystem {
        bag.insert(
            "mikebom:pe-subsystem".to_string(),
            serde_json::Value::String(subsystem.clone()),
        );
    }
    // Milestone 098 FR-003 — PE linker-version always-emit on
    // parseable PEs. `pe_linker_version` is `Some(...)` whenever
    // the PE parser succeeded (zero-zero → "0.0"); `None` only when
    // parse_pe_identity itself failed, in which case mikebom already
    // dropped the binary upstream.
    if let Some(ref lv) = scan.pe_linker_version {
        bag.insert(
            "mikebom:pe-linker-version".to_string(),
            serde_json::Value::String(lv.clone()),
        );
    }
    bag
}

/// Extension helper: attach the file-SHA-256 as a `hashes` field.
impl PackageDbEntry {
    fn with_sha256_placeholder(self, _hash: ContentHash) -> Self {
        // `PackageDbEntry` doesn't currently carry hashes directly;
        // hashes land on the `ResolvedComponent` via the scan_fs
        // conversion layer from the artefact-file walker. Binary
        // file-level components bypass that walker (they're produced
        // here), so a follow-on could extend `PackageDbEntry` with a
        // hashes field. For this turn, hashes on binary components
        // are omitted — consumers see the file-level component with
        // the filename + bom-ref identity but without content hashes.
        // Future: hook into the milestone-003 `file_hashes` plumbing.
        self
    }
}

/// Convert a parsed `.note.package` payload into a `PackageDbEntry`
/// per FR-024. Vendor derived from `distro` via the milestone-003
/// `rpm_vendor_from_id` map for RPM-family notes.
pub(super) fn note_package_to_entry(
    note: &elf::ElfNotePackage,
    path: &Path,
    os_release_id: Option<&str>,
    os_release_version_id: Option<&str>,
) -> Option<PackageDbEntry> {
    if note.name.is_empty() || note.version.is_empty() {
        return None;
    }
    let mut qualifiers = note
        .architecture
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|a| format!("?arch={a}"))
        .unwrap_or_default();

    // v6 fix (conformance bug 1 / ELF-note ghosts): vendor namespace
    // precedence is (1) the ELF note's own `distro` field when
    // populated, then (2) the scan-wide `/etc/os-release` ID, then
    // (3) a hardcoded default. Prior to this change, an unclaimed
    // Fedora binary with no `distro` in its ELF note emitted
    // `pkg:rpm/rpm/<name>@<ver>` — no OS context. Threading the
    // os-release ID recovers the correct namespace for the fallback
    // path.
    let resolve_vendor = |note_distro: Option<&str>, default_fallback: &str| -> String {
        let from_note = note_distro
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|d| d.to_lowercase());
        if let Some(d) = from_note {
            return d;
        }
        if let Some(id) = os_release_id.filter(|s| !s.is_empty()) {
            return id.to_lowercase();
        }
        default_fallback.to_string()
    };
    let append_distro_qualifier = |qualifiers: &mut String, vendor: &str| {
        // Emit `distro=<vendor>-<VERSION_ID>` only when both halves
        // are available. Mirrors the dpkg / rpm / apk package-db
        // readers' qualifier shape.
        if let Some(version_id) = os_release_version_id.filter(|s| !s.is_empty()) {
            let prefix = if qualifiers.is_empty() { '?' } else { '&' };
            qualifiers.push(prefix);
            qualifiers.push_str("distro=");
            qualifiers.push_str(vendor);
            qualifiers.push('-');
            qualifiers.push_str(version_id);
        }
    };

    // purl-spec § Character encoding: `+` and other non-allowed chars
    // MUST be percent-encoded in BOTH the name and version segments.
    // The note.{name,version} came out of an ELF `.note.package`
    // section and can carry real-world package coords with `+` (RPMs
    // like `libstdc++`, semver versions like `1.0+build.1`). Route
    // both through the canonical encoder so all five arms below emit
    // spec-conformant PURLs.
    let encoded_name = mikebom_common::types::purl::encode_purl_segment(&note.name);
    let encoded_version = mikebom_common::types::purl::encode_purl_segment(&note.version);
    let purl_str = match note.note_type.as_str() {
        "rpm" => {
            let raw_vendor = resolve_vendor(note.distro.as_deref(), "rpm");
            // rpm_vendor_from_id normalizes `rhel`→`redhat`, `ol`→`oracle`,
            // etc. Same mapping used by rpm.rs for the rpmdb reader.
            let vendor = rpm_vendor_from_id(&raw_vendor);
            append_distro_qualifier(&mut qualifiers, &vendor);
            format!("pkg:rpm/{vendor}/{encoded_name}@{encoded_version}{qualifiers}")
        }
        "deb" => {
            let vendor = resolve_vendor(note.distro.as_deref(), "debian");
            append_distro_qualifier(&mut qualifiers, &vendor);
            format!("pkg:deb/{vendor}/{encoded_name}@{encoded_version}{qualifiers}")
        }
        "apk" => {
            let vendor = resolve_vendor(note.distro.as_deref(), "alpine");
            append_distro_qualifier(&mut qualifiers, &vendor);
            format!("pkg:apk/{vendor}/{encoded_name}@{encoded_version}{qualifiers}")
        }
        "alpm" | "pacman" => {
            format!("pkg:alpm/arch/{encoded_name}@{encoded_version}{qualifiers}")
        }
        _ => format!("pkg:generic/{encoded_name}@{encoded_version}"),
    };

    let purl = Purl::new(&purl_str).ok()?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: note.name.clone(),
        version: note.version.clone(),
        arch: note.architecture.clone(),
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: vec![],
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: Some("elf-note-package".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: Default::default(),
        binary_role: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn note_package_rpm_produces_canonical_purl() {
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "curl".into(),
            version: "8.2.1".into(),
            architecture: Some("x86_64".into()),
            distro: Some("fedora".into()),
            os_cpe: None,
        };
        let entry =
            note_package_to_entry(&note, Path::new("/opt/curl"), None, None).unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:rpm/fedora/curl@8.2.1?arch=x86_64"
        );
        assert_eq!(entry.evidence_kind.as_deref(), Some("elf-note-package"));
        assert_eq!(entry.sbom_tier.as_deref(), Some("source"));
    }

    #[test]
    fn note_package_rpm_uses_os_release_namespace_when_note_distro_absent() {
        // Conformance bug 1 fix: when the ELF note has no distro field
        // but the scan's /etc/os-release ID is known, use the os-release
        // ID instead of the bare "rpm" fallback. Fixes Fedora ghosts
        // emitting pkg:rpm/rpm/<name>.
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "ModemManager".into(),
            version: "1.22.0-3.fc40".into(),
            architecture: Some("aarch64".into()),
            distro: None,
            os_cpe: None,
        };
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/libexec/mm-plugin-broadband"),
            Some("fedora"),
            Some("40"),
        )
        .unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:rpm/fedora/ModemManager@1.22.0-3.fc40?arch=aarch64&distro=fedora-40"
        );
    }

    #[test]
    fn note_package_rpm_prefers_note_distro_over_os_release() {
        // Precedence: ELF note's own `distro` wins over os-release ID.
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "curl".into(),
            version: "8.2.1".into(),
            architecture: Some("x86_64".into()),
            distro: Some("rocky".into()),
            os_cpe: None,
        };
        // Note says rocky, os-release (hypothetically wrong) says fedora.
        // rocky wins; rpm_vendor_from_id keeps rocky→rocky, then appends
        // distro=rocky-9 from VERSION_ID.
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/bin/curl"),
            Some("fedora"),
            Some("9"),
        )
        .unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:rpm/rocky/curl@8.2.1?arch=x86_64&distro=rocky-9"
        );
    }

    #[test]
    fn note_package_rpm_percent_encodes_plus_in_name() {
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "libstdc++".into(),
            version: "14.2.1-3.fc40".into(),
            architecture: Some("aarch64".into()),
            distro: Some("fedora".into()),
            os_cpe: None,
        };
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/lib64/libstdc++.so.6"),
            Some("fedora"),
            Some("40"),
        )
        .unwrap();
        let purl = entry.purl.as_str();
        assert!(
            purl.contains("/libstdc%2B%2B@"),
            "expected percent-encoded `++` in ELF-note PURL; got {purl}",
        );
        assert!(
            !purl.contains("libstdc++"),
            "literal `++` must not appear; got {purl}",
        );
    }

    #[test]
    fn note_package_rpm_percent_encodes_mid_name_plus() {
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "perl-Text-Tabs+Wrap".into(),
            version: "2024.001-1.fc40".into(),
            architecture: Some("noarch".into()),
            distro: Some("fedora".into()),
            os_cpe: None,
        };
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/share/perl5/Text/Tabs.pm"),
            Some("fedora"),
            Some("40"),
        )
        .unwrap();
        assert!(
            entry.purl.as_str().contains("/perl-Text-Tabs%2BWrap@"),
            "mid-name `+` must percent-encode; got {}",
            entry.purl.as_str()
        );
    }

    #[test]
    fn note_package_rpm_falls_back_to_rpm_when_no_context() {
        // Final fallback: no note distro, no os-release. Emits the
        // original bare "rpm" namespace. In practice this should never
        // happen on a real scan (os-release is read first), but the
        // defensive default preserves PURL validity.
        let note = elf::ElfNotePackage {
            note_type: "rpm".into(),
            name: "foo".into(),
            version: "1.0".into(),
            architecture: None,
            distro: None,
            os_cpe: None,
        };
        let entry =
            note_package_to_entry(&note, Path::new("/bin/foo"), None, None).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:rpm/rpm/foo@1.0");
    }

    #[test]
    fn note_package_alpm_uses_arch_namespace() {
        let note = elf::ElfNotePackage {
            note_type: "alpm".into(),
            name: "bash".into(),
            version: "5.2.015-1".into(),
            architecture: Some("x86_64".into()),
            distro: Some("Arch Linux".into()),
            os_cpe: None,
        };
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/bin/bash"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:alpm/arch/bash@5.2.015-1?arch=x86_64"
        );
    }

    #[test]
    fn note_package_deb_falls_back_to_debian_vendor() {
        let note = elf::ElfNotePackage {
            note_type: "deb".into(),
            name: "vim".into(),
            version: "9.0.0".into(),
            architecture: Some("amd64".into()),
            distro: None,
            os_cpe: None,
        };
        // No os-release context either → "debian" fallback.
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/bin/vim"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:deb/debian/vim@9.0.0?arch=amd64"
        );
    }

    #[test]
    fn note_package_deb_uses_os_release_namespace_for_ubuntu() {
        // Ubuntu image: ELF note lacks distro, os-release says ubuntu.
        let note = elf::ElfNotePackage {
            note_type: "deb".into(),
            name: "openssh-server".into(),
            version: "1:9.6p1-3ubuntu13".into(),
            architecture: Some("amd64".into()),
            distro: None,
            os_cpe: None,
        };
        let entry = note_package_to_entry(
            &note,
            Path::new("/usr/sbin/sshd"),
            Some("ubuntu"),
            Some("24.04"),
        )
        .unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:deb/ubuntu/openssh-server@1:9.6p1-3ubuntu13?arch=amd64&distro=ubuntu-24.04"
        );
    }

    #[test]
    fn note_package_unknown_type_becomes_generic() {
        let note = elf::ElfNotePackage {
            note_type: "xbps".into(),
            name: "foo".into(),
            version: "1.0".into(),
            architecture: None,
            distro: None,
            os_cpe: None,
        };
        let entry =
            note_package_to_entry(&note, Path::new("/bin/foo"), None, None).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:generic/foo@1.0");
    }

    fn fake_binary_scan() -> BinaryScan {
        BinaryScan {
            binary_class: "elf",
            imports: Vec::new(),
            has_dynamic: false,
            stripped: false,
            note_package: None,
            build_id: None,
            runpath: Vec::new(),
            debuglink: None,
            macho_uuid: None,
            macho_rpath: Vec::new(),
            macho_min_os: None,
            macho_codesign_identifier: None,
            macho_codesign_flags: Vec::new(),
            macho_codesign_team_id: None,
            pe_pdb_id: None,
            pe_machine: None,
            pe_subsystem: None,
            cargo_auditable: None,
            string_region: Vec::new(),
            packer: None,
            symbol_names: Vec::new(),
            comment_stamps: Vec::new(),
            macho_build_version: None,
            pe_linker_version: None,
            binary_role: mikebom_common::resolution::BinaryRole::Application,
        }
    }

    #[test]
    fn make_file_level_component_sets_detected_go_when_flag_set() {
        // G1 wiring: `make_file_level_component` receives
        // `detected_go = true` when the caller's `go_in_linux`
        // check fires. The emitted PackageDbEntry carries
        // `detected_go = Some(true)` so the CDX emitter surfaces
        // `mikebom:detected-go = true` on the file-level
        // component, cross-linking it with the sibling
        // `pkg:golang/.../module@version` entries from
        // `go_binary.rs`.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("goapp");
        std::fs::write(&path, b"dummy-bytes").unwrap();
        let scan = fake_binary_scan();
        let entry =
            make_file_level_component(&path, b"dummy-bytes", &scan, true);
        assert_eq!(entry.name, "goapp");
        assert_eq!(entry.detected_go, Some(true));
        assert_eq!(entry.binary_class.as_deref(), Some("elf"));
        assert!(
            entry.purl.as_str().starts_with("pkg:generic/goapp"),
            "expected pkg:generic/goapp PURL: {}",
            entry.purl.as_str(),
        );
    }

    #[test]
    fn make_file_level_component_leaves_detected_go_none_for_non_go() {
        // Regression guard: non-Go file-level entries (plain ELF,
        // Mach-O binaries without BuildInfo) keep `detected_go =
        // None` so the CDX property is only emitted when the
        // cross-link is real.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plain-tool");
        std::fs::write(&path, b"plain-bytes").unwrap();
        let scan = fake_binary_scan();
        let entry =
            make_file_level_component(&path, b"plain-bytes", &scan, false);
        assert_eq!(entry.detected_go, None);
    }

    /// Milestone 023: when BinaryScan carries all three ELF identity
    /// fields, the bag picks up exactly three entries with the
    /// expected key-value shapes.
    #[test]
    fn make_file_level_component_populates_bag_with_all_three_elf_signals() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("with-all");
        std::fs::write(&path, b"sample").unwrap();

        let mut scan = fake_binary_scan();
        scan.build_id = Some("deadbeef".repeat(5));
        scan.runpath = vec!["$ORIGIN/../lib".to_string(), "/opt/vendor/lib".to_string()];
        scan.debuglink = Some(elf::DebuglinkEntry {
            file: "with-all.debug".to_string(),
            crc32: 0xdeadbeef,
        });

        let entry = make_file_level_component(&path, b"sample", &scan, false);
        assert_eq!(entry.extra_annotations.len(), 3);
        assert_eq!(
            entry.extra_annotations.get("mikebom:elf-build-id"),
            Some(&serde_json::Value::String("deadbeef".repeat(5))),
        );
        assert_eq!(
            entry.extra_annotations.get("mikebom:elf-runpath"),
            Some(&serde_json::json!([
                "$ORIGIN/../lib",
                "/opt/vendor/lib"
            ])),
        );
        assert_eq!(
            entry.extra_annotations.get("mikebom:elf-debuglink"),
            Some(&serde_json::json!({
                "file": "with-all.debug",
                "crc32": "deadbeef",
            })),
        );
    }

    /// When BinaryScan carries no ELF identity fields, the bag is
    /// empty — no `mikebom:elf-*` annotations slip through.
    #[test]
    fn make_file_level_component_empty_bag_when_elf_signals_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plain");
        std::fs::write(&path, b"plain").unwrap();

        let scan = fake_binary_scan(); // all three fields default-empty
        let entry = make_file_level_component(&path, b"plain", &scan, false);
        assert!(entry.extra_annotations.is_empty());
    }

    /// Mixed case: only build-id present. The bag emits only the
    /// build-id key — no empty runpath array, no empty debuglink.
    #[test]
    fn make_file_level_component_bag_skips_unpopulated_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("with-build-id-only");
        std::fs::write(&path, b"only-build-id").unwrap();

        let mut scan = fake_binary_scan();
        scan.build_id = Some("c0ffee".to_string());
        // runpath stays empty; debuglink stays None.

        let entry = make_file_level_component(&path, b"only-build-id", &scan, false);
        assert_eq!(entry.extra_annotations.len(), 1);
        assert_eq!(
            entry.extra_annotations.get("mikebom:elf-build-id"),
            Some(&serde_json::Value::String("c0ffee".to_string())),
        );
        assert!(!entry.extra_annotations.contains_key("mikebom:elf-runpath"));
        assert!(!entry.extra_annotations.contains_key("mikebom:elf-debuglink"));
    }

    /// Milestone 028: a fully-populated PE BinaryScan emits all three
    /// PE bag keys with the expected values.
    #[test]
    fn make_file_level_component_populates_bag_with_all_three_pe_signals() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sample.exe");
        std::fs::write(&path, b"sample").unwrap();

        let mut scan = fake_binary_scan();
        scan.binary_class = "pe";
        scan.pe_pdb_id = Some("0123456789abcdeffedcba9876543210:7".to_string());
        scan.pe_machine = Some("amd64".to_string());
        scan.pe_subsystem = Some("console".to_string());

        let entry = make_file_level_component(&path, b"sample", &scan, false);
        assert_eq!(entry.extra_annotations.len(), 3);
        assert_eq!(
            entry.extra_annotations.get("mikebom:pe-pdb-id"),
            Some(&serde_json::Value::String(
                "0123456789abcdeffedcba9876543210:7".to_string(),
            )),
        );
        assert_eq!(
            entry.extra_annotations.get("mikebom:pe-machine"),
            Some(&serde_json::Value::String("amd64".to_string())),
        );
        assert_eq!(
            entry.extra_annotations.get("mikebom:pe-subsystem"),
            Some(&serde_json::Value::String("console".to_string())),
        );
    }

    /// Milestone 028: a partially-populated PE BinaryScan (e.g. a
    /// stripped exe with no CodeView record but valid machine +
    /// subsystem) emits only the populated bag keys.
    #[test]
    fn make_file_level_component_pe_bag_skips_unpopulated_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stripped.exe");
        std::fs::write(&path, b"stripped").unwrap();

        let mut scan = fake_binary_scan();
        scan.binary_class = "pe";
        // pe_pdb_id stays None (stripped binary).
        scan.pe_machine = Some("arm64".to_string());
        scan.pe_subsystem = Some("efi-application".to_string());

        let entry = make_file_level_component(&path, b"stripped", &scan, false);
        assert_eq!(entry.extra_annotations.len(), 2);
        assert!(!entry.extra_annotations.contains_key("mikebom:pe-pdb-id"));
        assert_eq!(
            entry.extra_annotations.get("mikebom:pe-machine"),
            Some(&serde_json::Value::String("arm64".to_string())),
        );
        assert_eq!(
            entry.extra_annotations.get("mikebom:pe-subsystem"),
            Some(&serde_json::Value::String("efi-application".to_string())),
        );
    }

    /// Milestone 029: a populated cargo-auditable manifest emits the
    /// `mikebom:detected-cargo-auditable = true` cross-link annotation
    /// on the file-level component.
    #[test]
    fn make_file_level_component_emits_detected_cargo_auditable_when_manifest_present() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rust-binary");
        std::fs::write(&path, b"sample").unwrap();

        let mut scan = fake_binary_scan();
        scan.cargo_auditable =
            Some(cargo_auditable::CargoAuditableManifest { packages: Vec::new() });

        let entry = make_file_level_component(&path, b"sample", &scan, false);
        assert_eq!(
            entry.extra_annotations.get("mikebom:detected-cargo-auditable"),
            Some(&serde_json::Value::Bool(true)),
        );
    }

    /// Milestone 029: when no manifest is present, the cross-link
    /// annotation does NOT emit (no false-true).
    #[test]
    fn make_file_level_component_omits_detected_cargo_auditable_when_manifest_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("not-rust-binary");
        std::fs::write(&path, b"plain").unwrap();

        let scan = fake_binary_scan(); // cargo_auditable = None

        let entry = make_file_level_component(&path, b"plain", &scan, false);
        assert!(!entry
            .extra_annotations
            .contains_key("mikebom:detected-cargo-auditable"));
    }

    /// Milestone 029: a 3-crate manifest emits 3 per-crate
    /// PackageDbEntry components with the expected fields, sorted
    /// deterministically by `(name, version, source)` triple.
    #[test]
    fn cargo_auditable_packages_to_entries_emits_per_crate_components() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rust-binary");
        std::fs::write(&path, b"sample").unwrap();

        let manifest = cargo_auditable::CargoAuditableManifest {
            packages: vec![
                cargo_auditable::CargoAuditablePackage {
                    name: "myapp".to_string(),
                    version: "1.2.3".to_string(),
                    source: "local".to_string(),
                    kind: Some("runtime".to_string()),
                    dependencies: vec![1, 2],
                    root: true,
                },
                cargo_auditable::CargoAuditablePackage {
                    name: "serde".to_string(),
                    version: "1.0.193".to_string(),
                    source: "registry".to_string(),
                    kind: Some("runtime".to_string()),
                    dependencies: Vec::new(),
                    root: false,
                },
                cargo_auditable::CargoAuditablePackage {
                    name: "tokio".to_string(),
                    version: "1.35.1".to_string(),
                    source: "registry".to_string(),
                    kind: Some("runtime".to_string()),
                    dependencies: Vec::new(),
                    root: false,
                },
            ],
        };

        let file_level_purl = mikebom_common::types::purl::Purl::new(
            "pkg:generic/rust-binary?file-sha256=deadbeef",
        )
        .unwrap();

        let entries =
            cargo_auditable_packages_to_entries(&manifest, &file_level_purl, &path);

        assert_eq!(entries.len(), 3);
        // Sorted by (name, version, source) → myapp, serde, tokio.
        assert_eq!(entries[0].name, "myapp");
        assert_eq!(entries[1].name, "serde");
        assert_eq!(entries[2].name, "tokio");

        // myapp is `local`-sourced → ?source=local qualifier on PURL.
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:cargo/myapp@1.2.3?source=local"
        );
        // serde + tokio are `registry`-sourced → no qualifier.
        assert_eq!(entries[1].purl.as_str(), "pkg:cargo/serde@1.0.193");
        assert_eq!(entries[2].purl.as_str(), "pkg:cargo/tokio@1.35.1");

        // Every entry shares the contracted evidence-kind + confidence.
        for e in &entries {
            assert_eq!(e.evidence_kind.as_deref(), Some("cargo-auditable"));
            assert_eq!(e.confidence.as_deref(), Some("high"));
            assert_eq!(
                e.parent_purl.as_deref(),
                Some("pkg:generic/rust-binary?file-sha256=deadbeef"),
            );
        }

        // myapp's `dependencies: [1, 2]` resolves to the serde+tokio
        // PURLs in sorted order.
        assert_eq!(
            entries[0].depends,
            vec![
                "pkg:cargo/serde@1.0.193".to_string(),
                "pkg:cargo/tokio@1.35.1".to_string()
            ]
        );

        // myapp carries the `mikebom:cargo-auditable-source = "local"`
        // annotation (non-registry sources surface). Runtime kind is
        // suppressed (the implied default).
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:cargo-auditable-source"),
            Some(&serde_json::Value::String("local".to_string())),
        );
        assert!(!entries[0]
            .extra_annotations
            .contains_key("mikebom:cargo-auditable-kind"));
    }
}
