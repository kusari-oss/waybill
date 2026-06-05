//! Generic (non-Go) binary SBOM reader — ELF in v1; Mach-O / PE
//! follow in later milestone-004 turns. Milestone 004 US2.
//!
//! Per-binary outputs (one file scanned → multiple `PackageDbEntry`
//! rows):
//! - One **file-level** binary component (`type=file`, carries
//!   `binary-class`, `binary-stripped`, `linkage-kind`).
//! - One **linkage-evidence** component per unique soname (deduped
//!   globally by PURL across the scan — see `linkage.rs`).
//! - One **ELF-note-package** component per binary that carries a
//!   `.note.package` section (source-tier, authoritative).
//!
//! Not yet emitted in this turn: embedded-version-string components
//! (T030), UPX packer detection (T031), Mach-O (T028/T034), PE
//! (T029/T035/T036). The `linkage::dedup_globally` pass runs at the
//! end of `read()` so cross-binary dedup happens before results leave
//! this module.

pub mod cargo_auditable;
pub mod elf;
pub(crate) mod fingerprints;
pub(crate) mod source_binding;
pub mod jdk_collapse;
pub mod linkage;
pub mod macho;
pub mod packer; // stub
pub mod pe;
pub mod python_collapse;
pub mod role;
pub mod symbol_fingerprint;
pub mod version_strings;

use std::path::Path;


use super::package_db::PackageDbEntry;

mod discover;
mod entry;
mod predicates;
mod scan;

use discover::discover_binaries;
use entry::{
    cargo_auditable_packages_to_entries, make_file_level_component, note_package_to_entry,
    symbol_match_to_entry, v2_match_to_entry, version_match_to_entry,
};
use predicates::{
    detect_rootfs_kind, has_rpmdb_at, is_host_system_path, is_os_managed_directory, RootfsKind,
};
use scan::{is_go_binary, scan_binary};

/// Check whether the walker's discovered path is covered by a claim
/// recorded by any installed-package-db reader.
///
/// Three independent matching layers, checked in order of cheapness:
/// 1. **Raw path match** — works on plain (non-usrmerge) rootfs
/// 2. **Canonical path match** — handles directory-level symlinks
///    (`/bin → usr/bin` in Debian usrmerge)
/// 3. **(device, inode) match** — handles hard links, final-component
///    symlinks, and canonicalize output-form differences
///
/// Layer 3 is the robust invariant: if walker path and any claim
/// point to the same physical file, their `(dev, ino)` match
/// regardless of how the path was constructed.
///
/// All three layers degrade to "not claimed" on `stat`/canonicalize
/// failure. Safe default: worst case a redundant `pkg:generic/`
/// component emits, matching pre-fix behaviour.
pub(crate) fn is_path_claimed(
    walker_path: &std::path::Path,
    claimed: &std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &std::collections::HashSet<(u64, u64)>,
) -> bool {
    // Layer 1: raw form — matches plain directory layouts.
    if claimed.contains(walker_path) {
        return true;
    }
    // Layer 2: canonical form — resolves symlinks on usrmerged rootfs.
    if let Ok(canonical) = std::fs::canonicalize(walker_path) {
        if claimed.contains(&canonical) {
            return true;
        }
    }
    // Layer 3: (device, inode) — handles hard links + any path-form
    // quirk canonicalize didn't normalise to the stored form.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(walker_path) {
            if claimed_inodes.contains(&(meta.dev(), meta.ino())) {
                return true;
            }
        }
    }
    false
}

/// Recursively scan `rootfs` for ELF / Mach-O / PE binaries, emit
/// file-level + linkage-evidence + ELF-note-package components, and
/// dedupe linkage evidence globally by PURL.
///
/// `claimed_paths` — files owned by an installed-package reader
/// (dpkg `.list`, apk `R:` lines, pip `RECORD`). Binaries whose paths
/// appear in this set skip their file-level + linkage emissions (the
/// owning package already accounts for them). `.note.package` +
/// embedded-version-string emissions remain unconditional — those
/// surface signals the package db can't produce (distro self-ID,
/// static TLS-library versions).
pub fn read(
    rootfs: &Path,
    claimed_paths: &std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &std::collections::HashSet<(u64, u64)>,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let mut linkage_agg = linkage::LinkageAggregator::new();
    let mut python_collapser = python_collapse::PythonStdlibCollapser::default();
    let mut jdk_collapser = jdk_collapse::JdkCollapser::default();
    let rootfs_kind = detect_rootfs_kind(rootfs);
    let has_rpmdb = has_rpmdb_at(rootfs);
    // Conformance bug 1 fix: the ELF-note-package PURL builder needs
    // the scan's OS context to fall back on when the note itself
    // doesn't carry a `distro` string. Read once per scan.
    let os_release_id = crate::scan_fs::os_release::read_id_from_rootfs(rootfs);
    let os_release_version_id =
        crate::scan_fs::os_release::read_version_id_from_rootfs(rootfs);
    // v5 Phase A diagnostic: when MIKEBOM_WALKER_DEBUG=1 is set, every
    // filter decision emits a single line to stderr. Used to identify
    // which binaries get dropped by which rule (regression-diagnosis
    // workflow on real fixtures). Gated to zero cost when unset.
    let walker_debug = std::env::var_os("MIKEBOM_WALKER_DEBUG").is_some();

    // Milestone 108 — load the external fingerprint corpus exactly
    // once per scan when the operator opted in via `--fingerprints-corpus`
    // (the clap derive sets `MIKEBOM_FINGERPRINTS_CORPUS=1`; the offline
    // signal rides the existing `MIKEBOM_OFFLINE` env var set in
    // main.rs). The bundled fallback path is the no-op default and
    // costs nothing — the per-binary loop calls the legacy `scan()`
    // wrapper which preserves SC-003 byte-identity. Hoisted out of the
    // loop so cache I/O (or the one-shot fetch) doesn't repeat per
    // binary.
    let fingerprints_opts = fingerprints::LoadOptions::from_env();
    let external_corpus = if fingerprints_opts.external_enabled {
        Some(fingerprints::load_corpus(fingerprints_opts.clone()))
    } else {
        None
    };

    // Milestone 110 Phase 4 Slice B-2 — load v2 records from the same
    // per-SHA cache directory the v1 path uses. Coexists with the v1
    // corpus: a single cache directory MAY contain a mix of v1 and v2
    // records (peek-at-schema_version dispatch lives in loader.rs).
    // When no v2 records are present (the current milestone-108 state),
    // this returns an empty Vec and the v2 matcher path is a no-op.
    let external_v2_records: Vec<fingerprints::record::CorpusRecordV2> =
        if external_corpus.is_some() {
            let sha = fingerprints_opts
                .sha_override
                .unwrap_or_else(fingerprints::source_sha::CorpusSha::build_time_embedded);
            fingerprints::loader::load_v2_records_from_cache(&sha).unwrap_or_default()
        } else {
            Vec::new()
        };
    let v2_source_id = fingerprints::source_config::CorpusSourceId::from_raw(
        fingerprints::source_config::CorpusSourceId::MILESTONE_108_DEFAULT.to_string(),
    );

    // Milestone 109 — build the source-tier attribution registry exactly
    // once per scan when the operator opted in via `--fingerprints-corpus`.
    // The registry maps (library_name_lc, scope_path) → cmake build-dir
    // observation; the per-binary matcher loop rewrites fingerprint-
    // match PURLs from `pkg:generic/<library>` to the source-tier PURL
    // when the binary lives under a cmake project that declared the
    // library via `FetchContent_Declare`. Empty registry (no cmake
    // projects in the scan root) is the no-op fallback — matcher
    // behavior is byte-identical to milestone 108 in that case.
    //
    // The cmake reader is re-invoked here because `binary::read()`
    // doesn't otherwise see the source-tier declarations from the
    // package_db layer. Cost: microseconds (regex + line iteration on
    // CMakeLists.txt files at depth 1 + Modules/ + third_party/).
    let attribution_registry = if external_corpus.is_some() {
        let cmake_decls = crate::scan_fs::package_db::cmake::read(rootfs, false);
        source_binding::build_attribution_registry(rootfs, &cmake_decls)
    } else {
        source_binding::BuildAttributionRegistry::from_observations(Vec::new())
    };

    for path in discover_binaries(rootfs) {
        let Ok(bytes) = std::fs::read(&path) else {
            if walker_debug {
                eprintln!("WALKER {}: DROPPED reason=read-error", path.display());
            }
            continue;
        };
        if bytes.len() < elf::MIN_BINARY_SIZE_BYTES as usize
            || bytes.len() > elf::MAX_BINARY_SIZE_BYTES as usize
        {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=size-out-of-bounds bytes={}",
                    path.display(),
                    bytes.len()
                );
            }
            continue;
        }
        let Some(scan) = scan_binary(&path, &bytes) else {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=scan-failed",
                    path.display()
                );
            }
            continue;
        };

        // OS-aware binary-format filter. Mach-O / PE binaries inside
        // a Linux container are nearly always contamination (test
        // fixtures, developer-host builds) whose linkage entries
        // point at host-OS paths. Skip them entirely to prevent the
        // SBOM from attributing `/System/Library/Frameworks/...`
        // dylibs to the container.
        if rootfs_kind == RootfsKind::Linux && scan.binary_class != "elf" {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=format-mismatch class={} rootfs=linux",
                    path.display(),
                    scan.binary_class
                );
            }
            continue;
        }
        if rootfs_kind == RootfsKind::Macos && scan.binary_class != "macho" {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=format-mismatch class={} rootfs=macos",
                    path.display(),
                    scan.binary_class
                );
            }
            continue;
        }
        if rootfs_kind == RootfsKind::Windows && scan.binary_class != "pe" {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=format-mismatch class={} rootfs=windows",
                    path.display(),
                    scan.binary_class
                );
            }
            continue;
        }

        // Milestone 004 post-ship double-counting fix. Suppress the
        // file-level + linkage-evidence emissions when the binary is
        // already owned by a package-db reader. `.note.package` remains
        // unconditional — it's authoritative distro self-identification.
        //
        // v2 fix: path_claimed now canonicalizes via `is_path_claimed`
        // so walker discoveries via /bin → usr/bin symlink traversal
        // (Debian usrmerge) correctly match claims recorded with the
        // canonical /usr/bin/... form. Pre-v2, the character-equality
        // lookup missed and 917/954 pkg:generic/ FPs leaked through.
        //
        // v6 fix (conformance bug 6a): embedded-version-string scans
        // were previously unconditional, which caused dpkg-owned
        // /usr/bin/curl to double-emit as `pkg:generic/curl@7.88.1`
        // alongside the dpkg `pkg:deb/.../curl@...` entry. The
        // deduplicator groups by (ecosystem, name, version) so the two
        // don't merge. Now gated on `skip_file_level_and_linkage` —
        // matches the same claim-aware skip that the file-level
        // emission uses. Trade-off: we lose static-library version
        // detection inside claimed binaries (e.g. statically-linked
        // OpenSSL in a dpkg-owned binary). Accepted because the FP
        // flood from self-identifying claimed binaries is the larger
        // correctness problem in practice.
        let path_claimed = is_path_claimed(
            &path,
            claimed_paths,
            #[cfg(unix)]
            claimed_inodes,
        );
        let rpm_dir_heuristic = rootfs_kind == RootfsKind::Linux
            && has_rpmdb
            && is_os_managed_directory(rootfs, &path);
        let go_in_linux =
            rootfs_kind == RootfsKind::Linux && is_go_binary(&bytes);

        // Python-stdlib collapse (v3 fix): when this binary matches
        // a CPython stdlib layout AND isn't already claimed by a
        // package-db reader, route it to the collapser instead of
        // emitting a file-level component. The collapser emits ONE
        // `pkg:generic/cpython@<X.Y>` umbrella per unique version at
        // scan end.
        let collapsed_by_python = !path_claimed
            && !rpm_dir_heuristic
            && !go_in_linux
            && python_collapser.try_collapse(&path, rootfs);

        // v5 Phase C: JDK umbrella collapse. Same pattern as Python —
        // one `pkg:generic/openjdk@<major>` umbrella per unique Java
        // version. Python gets first refusal (cheap, unlikely to match
        // JDK paths but belt-and-suspenders).
        let collapsed_by_jdk = !path_claimed
            && !rpm_dir_heuristic
            && !go_in_linux
            && !collapsed_by_python
            && jdk_collapser.try_collapse(&path, rootfs);

        // v4 Fix 3 — object files (.o) and static archives (.a) are
        // compilation intermediates, not runtime components. After the
        // Python collapser has had a chance to route them into the
        // cpython umbrella (for Python-<ver>/ source trees), any
        // remaining .o/.a gets silently dropped.
        //
        // Real static archives carry magic `!<arch>\n` and are
        // rejected upstream by `is_supported_binary` — so in practice
        // the `.a` arm only catches the edge case of an ELF file
        // misnamed with a `.a` extension (seen in some build
        // pipelines). Kept as defense-in-depth.
        let is_build_intermediate = !collapsed_by_python
            && matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("o") | Some("a")
            );
        if is_build_intermediate {
            if walker_debug {
                eprintln!(
                    "WALKER {}: DROPPED reason=build-intermediate ext={:?}",
                    path.display(),
                    path.extension().and_then(|e| e.to_str())
                );
            }
            continue;
        }

        if walker_debug {
            if path_claimed {
                eprintln!(
                    "WALKER {}: SKIPPED reason=path-claimed",
                    path.display()
                );
            } else if rpm_dir_heuristic {
                eprintln!(
                    "WALKER {}: SKIPPED reason=rpm-dir-heuristic",
                    path.display()
                );
            } else if go_in_linux {
                // G1: Go binaries now emit file-level (with
                // detected_go = Some(true)) but still skip
                // secondary evidence (linkage, ELF-note,
                // version-strings).
                eprintln!(
                    "WALKER {}: EMITTED file-level class={} detected_go=true (secondary-evidence suppressed)",
                    path.display(),
                    scan.binary_class
                );
            } else if collapsed_by_python {
                eprintln!(
                    "WALKER {}: COLLAPSED-PYTHON",
                    path.display()
                );
            } else if collapsed_by_jdk {
                eprintln!(
                    "WALKER {}: COLLAPSED-JDK",
                    path.display()
                );
            } else {
                eprintln!(
                    "WALKER {}: EMITTED file-level class={}",
                    path.display(),
                    scan.binary_class
                );
            }
        }

        // G1: split the skip gate. Go binaries on Linux still skip
        // secondary evidence emission (DT_NEEDED linkage, ELF-note
        // package, embedded-version-string scanner), but they
        // SHOULD emit the file-level `pkg:generic/<name>?file-sha256=...`
        // component. The ground truth counts the binary artifact
        // itself as a component alongside its embedded
        // `pkg:golang/<module>@<ver>` identities (emitted separately
        // by go_binary.rs) — same dual-identity pattern as the
        // Maven JAR ↔ RPM case.
        //
        // `skip_file_level`: claimed / rpm-dir / python / jdk
        // collapse cases ONLY. Go binaries no longer skip here.
        //
        // `skip_secondary_evidence`: `skip_file_level` ∪ `go_in_linux`.
        // Statically-linked Go binaries produce minimal DT_NEEDED
        // (just libc); emitting per-library linkage components
        // would inflate the SBOM with noise. ELF-note and
        // version-string scanners are also suppressed for Go —
        // their output is redundant with the golang module
        // emission from `go_binary.rs`.
        let skip_file_level = path_claimed
            || rpm_dir_heuristic
            || collapsed_by_python
            || collapsed_by_jdk;
        let skip_secondary_evidence = skip_file_level || go_in_linux;

        // The file-level component's PURL is needed both for the
        // skip_file_level==false push below AND for the milestone-029
        // cargo-auditable per-crate components' parent_purl
        // cross-link (which emits regardless of skip_file_level —
        // those crates are real, even when the file-level component
        // is shadowed by an authoritative package-db entry). Build
        // it once outside the skip branch.
        let file_level = make_file_level_component(&path, &bytes, &scan, go_in_linux);
        let file_level_purl = file_level.purl.clone();

        if !skip_file_level {
            // File-level binary component. When `go_in_linux` is
            // true, the emitted entry carries `detected_go = Some(true)`
            // to cross-link it with the golang module component(s)
            // that `go_binary.rs` emits for the same bytes
            // (milestone 004 US2 R8 flat cross-link).
            let parent_bom_ref = file_level.purl.as_str().to_string();
            out.push(file_level);

            // Linkage-evidence components — accumulated into the global
            // dedup aggregator; emitted after the walk completes.
            // Host-system-path install-names filtered out to prevent
            // `/System/Library/Frameworks/...` leakage.
            //
            // v6 (conformance bug 6b): `add_with_claim_check` probes
            // standard library search paths and skips sonames that
            // resolve to a path claimed by a package-db reader.
            // Fixes `libc.so.6` double-emission alongside the libc6
            // deb.
            //
            // G1: `skip_secondary_evidence` adds `go_in_linux` on top of the
            // file-level gates. Go binaries are statically linked by
            // default so their DT_NEEDED set is tiny (libc only) —
            // not worth inflating the SBOM with per-library linkage
            // components. The file-level `pkg:generic/...` component
            // plus the `pkg:golang/.../module@version` components
            // from `go_binary.rs` give the right granularity.
            if !skip_secondary_evidence {
                for soname in &scan.imports {
                    if is_host_system_path(soname) {
                        continue;
                    }
                    linkage_agg.add_with_claim_check(
                        soname,
                        &path,
                        &parent_bom_ref,
                        rootfs,
                        claimed_paths,
                        #[cfg(unix)]
                        claimed_inodes,
                    );
                }
            }
        }

        // ELF-note-package component (authoritative, source-tier;
        // ELF-only — Mach-O / PE don't carry this section).
        //
        // v6 fix (conformance bug 1): gated on `skip_file_level_and_linkage`
        // so claimed binaries (dpkg/rpm/apk-owned) don't double-emit
        // `pkg:rpm/rpm/<source-package>@<ver>` ghosts alongside the
        // authoritative `pkg:rpm/<vendor>/<deployed-subpackage>@<ver>`
        // entry from the package-db reader. Fedora images previously
        // produced 50 such ghosts. Unclaimed binaries still emit —
        // this is the only identity source for them.
        if !skip_secondary_evidence {
            if let Some(note) = &scan.note_package {
                if let Some(note_entry) = note_package_to_entry(
                    note,
                    &path,
                    os_release_id.as_deref(),
                    os_release_version_id.as_deref(),
                ) {
                    out.push(note_entry);
                }
            }
        }

        // Curated embedded-version-string scanner per FR-025 / R6
        // PLUS milestone-096 FR-004 symbol-fingerprint scanner.
        // Confined to read-only string sections (versions) and ELF
        // `.dynsym` (symbols) — both attempt to identify libraries
        // statically linked into unknown binaries.
        //
        // Per milestone-096 Clarification Q1: when BOTH techniques fire
        // for the same library on the same binary, merge into ONE
        // `PackageDbEntry` with both evidence trails. The version-string
        // entry's PURL (with `@<version>`) wins because it carries
        // higher-confidence identity; the symbol-fingerprint signal is
        // recorded as a corroborating annotation
        // (`mikebom:fingerprint-symbols-matched`) on the same component.
        //
        // v6: gated on `skip_file_level_and_linkage` so claimed
        // binaries (dpkg/rpm-owned, collapsed-by-python/jdk, go
        // binaries on Linux) don't double-emit `pkg:generic/curl`
        // alongside the package-db scanner's authoritative entry.
        if !skip_secondary_evidence {
            // Per-binary, per-library composite-evidence collector.
            // Key: lowercase library name (matches both `version_strings`
            // `CuratedLibrary::slug()` and `symbol_fingerprint`'s
            // `library` field — both already lowercase).
            let mut by_library: std::collections::HashMap<
                String,
                PackageDbEntry,
            > = std::collections::HashMap::new();

            for m in version_strings::scan(&scan.string_region) {
                if let Some(entry) = version_match_to_entry(&m, &path) {
                    by_library.insert(m.library.slug().to_string(), entry);
                }
            }
            // Milestone 108 — when the operator opted in to the
            // external corpus, run the matcher against it (stamping
            // `mikebom:fingerprint-corpus-sha` on every emitted match
            // per FR-005). Otherwise stick with the bundled path
            // through the legacy `scan()` wrapper — preserves the
            // SC-003 byte-identity contract for the 33 byte-identity
            // goldens.
            let symbol_matches = match &external_corpus {
                Some(corpus) => symbol_fingerprint::scan_with_corpus(
                    &scan.symbol_names,
                    corpus,
                    true,
                    // Milestone 109 — feed the attribution registry +
                    // this binary's path so the matcher can rewrite
                    // generic PURLs to source-tier PURLs when the
                    // cmake build-dir observation matches.
                    Some(&attribution_registry),
                    Some(&path),
                ),
                None => symbol_fingerprint::scan(&scan.symbol_names),
            };
            for m in symbol_matches {
                // Q1: if the version-string scanner already produced an
                // entry for this library on this binary, just record the
                // symbol-fingerprint corroboration on that entry. The
                // higher-confidence version-string PURL pins identity.
                if let Some(existing) = by_library.get_mut(&m.library) {
                    existing.extra_annotations.insert(
                        "mikebom:fingerprint-symbols-matched".to_string(),
                        serde_json::Value::String(format!(
                            "{}/{}",
                            m.matched_count, m.total_count
                        )),
                    );
                    // Milestone 108: the corpus-sha annotation rides
                    // along on the corroboration path so the SBOM still
                    // attributes the symbol-fingerprint signal to a
                    // specific corpus when the operator is opted in.
                    //
                    // Milestone 110 FR-017 (US3): when corpus-sha is
                    // emitted, ALSO emit the numeric fused-confidence
                    // value as `mikebom:fingerprint-confidence`. Distinct
                    // from the existing C16 `mikebom:confidence` enum
                    // (value=`"heuristic"`) so no carrier collision.
                    if let Some(ref sha) = m.corpus_sha_annotation {
                        existing.extra_annotations.insert(
                            "mikebom:fingerprint-corpus-sha".to_string(),
                            serde_json::Value::String(sha.clone()),
                        );
                        existing.extra_annotations.insert(
                            "mikebom:fingerprint-confidence".to_string(),
                            serde_json::Value::String("0.70".to_string()),
                        );
                    }
                    continue;
                }
                let library_key = m.library.clone();
                if let Some(entry) = symbol_match_to_entry(&m, &path) {
                    by_library.insert(library_key, entry);
                }
            }

            // Milestone 110 Phase 4 Slice B-2 — v2 matcher pass.
            //
            // After the v1 path has settled, run the v2 matcher against
            // any v2 records the cache loaded. v2 records carry typed
            // multi-indicator specs + canonical (often versioned) PURLs;
            // each non-suppressed match becomes a PackageDbEntry via
            // entry::v2_match_to_entry.
            //
            // CRITICAL ordering: v2 results are merged AFTER v1 + only
            // for libraries the v1 path didn't already cover (the
            // `entry()`-vacant gate below). This preserves
            // milestone-108 byte-identity for the 33 existing
            // byte-identity goldens — a v2 record that happens to
            // share a library name with a v1 record does NOT override
            // the v1 emission. Cross-tier collision handling +
            // mikebom:also-detected-via cross-references ship in a
            // follow-on slice (Phase 6 / US4).
            if !external_v2_records.is_empty() {
                let artifact = fingerprints::v2_bridge::binary_artifact_from_scan(
                    &scan.symbol_names,
                    &scan.string_region,
                    scan.build_id.as_deref(),
                    scan.macho_uuid.as_deref(),
                    scan.pe_pdb_id.as_deref(),
                );
                let v2_results = fingerprints::matcher::match_binary(
                    &artifact,
                    &external_v2_records,
                    None, // self-identity ladder lands Phase 6.
                    &v2_source_id,
                );
                for r in v2_results {
                    let key = r.purl.name().to_lowercase();
                    by_library.entry(key).or_insert_with(|| v2_match_to_entry(&r, &path));
                }
            }

            // Deterministic order: sort by PURL string so cross-run
            // golden bytes stay stable.
            let mut new_entries: Vec<PackageDbEntry> =
                by_library.into_values().collect();
            new_entries.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
            out.extend(new_entries);
        }

        // Milestone 029 — cargo-auditable per-crate components.
        // Same `skip_secondary_evidence` gate as the version-string
        // scanner: when the binary is already covered by an
        // authoritative package-db entry (dpkg/rpm/etc.), don't
        // double-emit `pkg:cargo/<crate>` shadows. Each emitted
        // crate carries `parent_purl = file_level_purl` cross-
        // linking back to the file-level binary component's identity.
        if !skip_secondary_evidence {
            if let Some(ref manifest) = scan.cargo_auditable {
                let entries = cargo_auditable_packages_to_entries(
                    manifest,
                    &file_level_purl,
                    &path,
                );
                out.extend(entries);
            }
        }
    }

    out.extend(linkage_agg.into_entries());
    out.extend(python_collapser.into_entries());
    out.extend(jdk_collapser.into_entries());
    out
}





#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;




    /// purl-spec § Character encoding: `+` in the ELF-note name must
    /// percent-encode to `%2B` just like the rpmdb path. Mirror of the
    /// regression tests in `scan_fs::package_db::rpm::tests`.







    #[test]
    fn empty_rootfs_yields_zero_binary_components() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(
            dir.path(),
            &Default::default(),
            #[cfg(unix)]
            &Default::default()
        )
        .is_empty());
    }

    #[test]
    fn non_elf_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("script.sh"), b"#!/bin/sh\necho hi").unwrap();
        std::fs::write(dir.path().join("data.txt"), b"hello world").unwrap();
        assert!(read(
            dir.path(),
            &Default::default(),
            #[cfg(unix)]
            &Default::default()
        )
        .is_empty());
    }


    /// Regression test for the Docker-image usrmerge failure mode
    /// (reported: 917 of 954 `pkg:generic/` FPs had basename matches in
    /// dpkg `.list` but missed the path-containment check).
    ///
    /// Reproduces the exact mismatch: walker discovers the binary via
    /// a symlinked path (`/rootfs/bin/base64`), claim was recorded as
    /// the canonical path (`/rootfs/usr/bin/base64`) via the
    /// `insert_claim_with_canonical` helper (matching how the real
    /// dpkg / apk / pip readers populate the claim set). The
    /// `is_path_claimed` lookup at walker time MUST recognise the
    /// two paths refer to the same inode via canonicalization.
    #[cfg(unix)]
    #[test]
    fn claim_skip_recognizes_usrmerge_symlink_path() {
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Real /usr/bin directory with a dummy binary.
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(root.join("usr/bin/base64"), b"not a real binary").unwrap();
        // /bin → usr/bin symlink (Debian usrmerge).
        std::os::unix::fs::symlink("usr/bin", root.join("bin")).unwrap();

        // Claim inserted via the production helper — dual-inserts raw
        // (rootfs.join("usr/bin/base64")) + parent-canonical forms so
        // the HashSet contains both representations the walker might
        // produce. Also records (dev, inode) for symlink-robust match.
        let mut claimed: HashSet<std::path::PathBuf> = HashSet::new();
        let mut inodes: HashSet<(u64, u64)> = HashSet::new();
        crate::scan_fs::package_db::insert_claim_with_canonical(
            &mut claimed,
            &mut inodes,
            root.join("usr/bin/base64"),
        );

        // Walker discovers the binary via the symlinked path.
        let walker_path = root.join("bin/base64");
        assert!(
            walker_path.exists(),
            "walker path must resolve via symlink"
        );
        assert_ne!(
            walker_path,
            root.join("usr/bin/base64"),
            "pre-canonicalization, walker path must differ from claim path"
        );

        // The claim-skip mechanism must recognise these as the same
        // via canonicalization on the walker side + the dual-insert
        // on the claim side.
        assert!(
            is_path_claimed(
                &walker_path,
                &claimed,
                #[cfg(unix)]
                &inodes
            ),
            "usrmerge: walker path via symlink MUST be recognised as claimed. \
             walker={walker_path:?}, claimed={claimed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn claim_skip_without_symlink_still_works() {
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(root.join("usr/bin/cat"), b"not a real binary").unwrap();

        let mut claimed: HashSet<std::path::PathBuf> = HashSet::new();
        let inodes: HashSet<(u64, u64)> = HashSet::new();
        claimed.insert(root.join("usr/bin/cat"));

        let walker_path = root.join("usr/bin/cat");
        assert!(
            is_path_claimed(&walker_path, &claimed, &inodes),
            "plain (non-usrmerge) claim match must still work"
        );
    }

    #[cfg(unix)]
    #[test]
    fn claim_skip_broken_symlink_does_not_panic() {
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Symlink pointing at a nonexistent target — canonicalize fails.
        std::os::unix::fs::symlink("does-not-exist", root.join("dangling")).unwrap();

        let claimed: HashSet<std::path::PathBuf> = HashSet::new();
        let inodes: HashSet<(u64, u64)> = HashSet::new();
        let walker_path = root.join("dangling");
        // Must not panic; returns false (not claimed → file would
        // be processed if it were a valid binary, which it isn't).
        assert!(!is_path_claimed(&walker_path, &claimed, &inodes));
    }

    /// Test A1 from v3 plan — inode match catches a final-component
    /// symlink even when canonicalize's output form differs from the
    /// stored claim.
    #[cfg(unix)]
    #[test]
    fn claim_skip_via_inode_on_symlinked_library() {
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("usr/lib")).unwrap();
        std::fs::write(root.join("usr/lib/libfoo.so.1"), b"dummy").unwrap();
        // Symlink libfoo.so → libfoo.so.1 in the same directory.
        std::os::unix::fs::symlink("libfoo.so.1", root.join("usr/lib/libfoo.so")).unwrap();

        // Claim only the real file.
        let mut claimed: HashSet<std::path::PathBuf> = HashSet::new();
        let mut inodes: HashSet<(u64, u64)> = HashSet::new();
        crate::scan_fs::package_db::insert_claim_with_canonical(
            &mut claimed,
            &mut inodes,
            root.join("usr/lib/libfoo.so.1"),
        );

        // Walker discovers the symlink path.
        let walker_path = root.join("usr/lib/libfoo.so");
        assert!(walker_path.exists());

        // Must recognize the symlink as claimed (via inode — canonicalize
        // also works here, but inode is the robust fallback that
        // closes the class of bug for more exotic symlink situations).
        assert!(
            is_path_claimed(&walker_path, &claimed, &inodes),
            "symlink library MUST be recognized as claimed. \
             walker={walker_path:?}, claimed={claimed:?}, inodes={inodes:?}"
        );
    }

    /// Test A2 from v3 plan — hard link. Canonicalize CANNOT collapse
    /// hard links (different directory entries for the same inode).
    /// Inode match is the only robust path.
    #[cfg(unix)]
    #[test]
    fn inode_match_survives_hard_link() {
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(root.join("usr/bin/a"), b"dummy").unwrap();
        // Hard link a → b in the same directory.
        std::fs::hard_link(root.join("usr/bin/a"), root.join("usr/bin/b")).unwrap();

        // Claim only `a`.
        let mut claimed: HashSet<std::path::PathBuf> = HashSet::new();
        let mut inodes: HashSet<(u64, u64)> = HashSet::new();
        crate::scan_fs::package_db::insert_claim_with_canonical(
            &mut claimed,
            &mut inodes,
            root.join("usr/bin/a"),
        );

        // Walker discovers `b` — different path, same inode.
        let walker_path = root.join("usr/bin/b");
        assert!(walker_path.exists());
        assert!(
            !claimed.contains(&walker_path),
            "raw path lookup must miss (hard link not path-equal)"
        );

        // Inode match is the only path that works here.
        assert!(
            is_path_claimed(&walker_path, &claimed, &inodes),
            "hard link MUST be recognized as claimed via inode match. \
             walker={walker_path:?}, inodes={inodes:?}"
        );
    }
}
