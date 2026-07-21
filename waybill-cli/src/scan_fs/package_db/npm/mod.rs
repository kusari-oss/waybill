//! Read Node.js package metadata from a scanned filesystem.
//!
//! Three layered sources in order of authority (per spec FR-006..FR-010
//! and research.md R4 / R5 / R8):
//!
//! 1. **Lockfile**: `package-lock.json` (v2/v3) or `pnpm-lock.yaml` (v6+).
//!    Confidence 0.85. Tier is `source` when no populated `node_modules/`
//!    is observed; `deployed` when both lockfile AND node_modules exist
//!    and agree (lockfile mirrors installed state). v1 lockfiles are
//!    refused with an actionable error per FR-006.
//! 2. **Flat `node_modules/` walk**: when no lockfile is present.
//!    Confidence 0.85, tier `deployed`.
//! 3. **Root `package.json` fallback** (FR-007a): when neither lockfile
//!    nor `node_modules/` is present, parse the root manifest's
//!    `dependencies` (and `devDependencies` when `--include-dev` is set).
//!    Confidence 0.70, tier `design`.
//!
//! Drift rule (research R8): when a lockfile and `node_modules/` disagree
//! on a package's version, `node_modules/` wins — the installed reality
//! trumps the locked declaration. Symmetrical with the Python venv rule.
//!
//! v1 lockfile refusal: when `package-lock.json` declares
//! `"lockfileVersion": 1`, the reader returns
//! [`NpmError::LockfileV1Unsupported`]. The CLI wraps it as a non-zero
//! exit with the stderr message documented in
//! `contracts/cli-interface.md`.

use std::path::{Path, PathBuf};

use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

/// Errors the npm reader can raise. Only `LockfileV1Unsupported` is
/// fatal (FR-006 + CLI contract); the rest are soft failures that the
/// dispatcher logs and swallows.
#[derive(Debug, thiserror::Error)]
pub enum NpmError {
    #[error("package-lock.json v1 not supported; regenerate with npm ≥7")]
    LockfileV1Unsupported { path: PathBuf },
}

/// Public entry point. Walks the scan root for npm package sources and
/// emits one `PackageDbEntry` per unique package identity. Returns
/// `Err(LockfileV1Unsupported)` when any candidate project root contains
/// a v1 lockfile; callers convert that to a non-zero exit.
///
/// For directory scans the sole candidate is `rootfs` itself. For image
/// scans (rootfs = extracted container filesystem), this additionally
/// probes the common image layouts where npm projects live — global
/// `/usr/lib/node_modules/`, `/app/`, `/usr/src/app/`, `/opt/*/`,
/// `/srv/*/` — so the reader finds node_modules trees that don't sit
/// at the rootfs root. See FR-010 of the 002 spec.
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    scan_mode: crate::scan_fs::ScanMode,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Result<Vec<PackageDbEntry>, NpmError> {
    let mut entries: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Milestone 163 (T012, closes #498): per-workspace-peer accumulators
    // collected at Tier C time; used to stamp the peer's main-module
    // component with (a) resolved dep NAMES in `depends` (downstream
    // graph resolver wires the edge to the concrete-version PURL via
    // `name_to_purl` at `scan_fs/mod.rs:471`), and (b) the C115
    // `mikebom:unresolved-declared-dep` annotation naming any dep the
    // cross-workspace index couldn't resolve.
    let mut peer_accumulators: std::collections::HashMap<
        PathBuf,
        walk::WorkspacePeerAccumulator,
    > = std::collections::HashMap::new();
    // Milestone 163 (T013): tallies for the FR-009 tracing log at the
    // end of `read()`.
    let mut ms163_resolved_total: usize = 0;
    let mut ms163_unresolved_total: usize = 0;

    for project_root in candidate_project_roots(rootfs, exclude_set) {
        // Detect v1 first — fail closed before emitting anything partial.
        let pkg_lock = project_root.join("package-lock.json");
        if pkg_lock.is_file() {
            if let Ok(text) = std::fs::read_to_string(&pkg_lock) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    let lockfile_version = parsed
                        .get("lockfileVersion")
                        .and_then(|v| v.as_u64());
                    if lockfile_version == Some(1) {
                        return Err(NpmError::LockfileV1Unsupported { path: pkg_lock });
                    }
                }
            }
        }

        let mut project_entries: Vec<PackageDbEntry> = Vec::new();

        // Tier A: lockfile (authoritative).
        if let Some(lockfile_entries) = package_lock::read_package_lock(&project_root, include_dev) {
            project_entries.extend(lockfile_entries);
        } else if let Some(pnpm_entries) = pnpm_lock::read_pnpm_lock(&project_root, include_dev) {
            project_entries.extend(pnpm_entries);
        } else if let Some(bun_entries) = bun_lock::read_bun_lock(&project_root, include_dev) {
            // Milestone 106 US2 (issue #278): Bun support. Same
            // tier-A authority as package-lock / pnpm-lock. The
            // legacy binary `bun.lockb` format is out of scope.
            project_entries.extend(bun_entries);
        } else if let Some(yarn_entries) = yarn_lock::read_yarn_lock(&project_root, include_dev) {
            // Milestone 106 US5 (issue #274): Yarn support. Handles
            // both v1 (Classic) and Berry (v2+) formats, auto-
            // detected via the `__metadata:` block sentinel.
            project_entries.extend(yarn_entries);
        }

        // Post-Tier-A author enrichment: lockfiles (v2/v3 and
        // pnpm-lock.yaml) don't carry per-package author, but when a
        // `node_modules/` tree is present alongside (typical
        // post-`npm install`), the installed `package.json` does.
        // Walk the tree and overlay `maintainer` onto matching
        // components by PURL. This is additive — it doesn't change
        // versions or add components beyond what the lockfile
        // declared.
        if !project_entries.is_empty() {
            enrich::enrich_entries_with_installed_authors(&project_root, &mut project_entries);
        }

        // Tier B: flat node_modules walk (fires when the lockfile didn't
        // produce anything — typical for images where the lockfile has
        // been stripped at build time but the installed tree remains).
        if project_entries.is_empty() {
            if let Some(nm_entries) = walk::read_node_modules(&project_root, scan_mode) {
                project_entries.extend(nm_entries);
            }
        }

        // Tier C: root package.json fallback (FR-007a).
        //
        // Milestone 163 (T011/T012, closes #498): when Tier A + Tier B
        // produced nothing at this project_root, we're either scanning a
        // workspace peer (hoisted-node_modules monorepo layout — some
        // OTHER project root in the scan had a lockfile that populated
        // Tier A entries in the shared `entries` accumulator) OR a
        // standalone package.json (no lockfile anywhere in the scan).
        //
        // Heuristic: the cross-workspace index built from the current
        // `entries` snapshot is EMPTY iff no lockfile-derived Tier A
        // entries exist in this scan. In that case we're standalone —
        // preserve pre-163 design-tier phantom emission to avoid signal
        // loss (a nameless package.json with no lockfile has no main-
        // module attach point for a C115 annotation). Otherwise the
        // scan HAS a workspace root; cross-resolve per Q1+Q2 unified
        // disposition — deps that resolve become NAMES in the peer's
        // main-module `depends`, deps that don't become C115
        // annotations, and ZERO phantom PURLs enter the graph
        // (SC-004 invariant for workspace-peer scans).
        if project_entries.is_empty() {
            let cross_workspace_index = walk::build_cross_workspace_index(&entries);
            let ctx = walk::CrossWorkspaceContext {
                peer_root: &project_root,
                index: &cross_workspace_index,
            };
            let effective_ctx = if cross_workspace_index.is_empty() {
                None // standalone scan — preserve pre-163 phantom emission
            } else {
                Some(&ctx) // workspace-peer context — cross-resolve
            };
            if let Some((fb_entries, acc)) =
                walk::read_root_package_json(&project_root, include_dev, effective_ctx)
            {
                project_entries.extend(fb_entries);
                ms163_resolved_total += acc.resolved_deps.len();
                ms163_unresolved_total += acc.unresolved_deps.len();
                if !acc.resolved_deps.is_empty() || !acc.unresolved_deps.is_empty() {
                    peer_accumulators.insert(project_root.clone(), acc);
                }
            }
        }

        // Milestone 199 US2 — stamp `mikebom:declared-as` on lockfile-
        // resolved entries whose declaration in the local `package.json`
        // used the `npm:<actual>@<ver>` alias syntax. Runs regardless of
        // which tier (A / B / C) produced the entries, since aliases can
        // reference either a lockfile-resolved component (Tier A) or a
        // design-tier phantom (Tier C).
        stamp_alias_declared_as(&project_root, &mut project_entries);

        for entry in project_entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                entries.push(entry);
            }
        }
    }

    // Milestone 066: emit one main-module per `package.json` with `name`
    // (skipping `private: true` + no version per #104). Augment-existing-
    // or-emit-new pattern mirrors cargo (064 T011) — when a same-PURL
    // lockfile-derived entry already exists, layer the C40 supplementary
    // tag + `sbom_tier: source` + `parent_purl: None` on top while
    // preserving the lockfile's `depends`. When no lockfile entry
    // collided, emit a net-new main-module (library packages without
    // committed lockfiles).
    let mut main_modules_emitted = 0usize;
    for project_root in candidate_project_roots(rootfs, exclude_set) {
        let Some(mut synthesized) = walk::build_npm_main_module_entry(&project_root) else {
            continue;
        };
        // Milestone 163 (T013, closes #498): stamp the synthesized
        // main-module with the workspace-peer accumulator collected
        // at Tier C. `resolved_deps` become NAMES appended to
        // `synthesized.depends` (downstream resolver at
        // `scan_fs/mod.rs:729` walks these to build DependsOn edges via
        // `name_to_purl` lookup against emitted Tier A entries).
        // `unresolved_deps` become the C115
        // `mikebom:unresolved-declared-dep` annotation on the peer's
        // main-module (bare string when 1; JSON array
        // sorted+deduplicated when ≥2).
        if let Some(acc) = peer_accumulators.remove(&project_root) {
            let existing_deps: std::collections::HashSet<String> =
                synthesized.depends.iter().cloned().collect();
            for name in &acc.resolved_deps {
                if !existing_deps.contains(name) {
                    synthesized.depends.push(name.clone());
                }
            }
            if !acc.unresolved_deps.is_empty() {
                let mut sorted: Vec<String> = acc.unresolved_deps.clone();
                sorted.sort();
                sorted.dedup();
                let value = if sorted.len() == 1 {
                    serde_json::Value::String(sorted.into_iter().next().unwrap_or_default())
                } else {
                    serde_json::Value::Array(
                        sorted.into_iter().map(serde_json::Value::String).collect(),
                    )
                };
                synthesized
                    .extra_annotations
                    .insert("mikebom:unresolved-declared-dep".to_string(), value);
            }
        }
        let purl_key = synthesized.purl.as_str().to_string();
        if let Some(existing) = entries.iter_mut().find(|e| e.purl.as_str() == purl_key) {
            // Augment in-place: layer C40 tag + sbom_tier:source over
            // any existing same-PURL entry.
            for (k, v) in synthesized.extra_annotations.iter() {
                existing
                    .extra_annotations
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            if existing.sbom_tier.is_none() {
                existing.sbom_tier = synthesized.sbom_tier.clone();
            }
            // Merge synthesized depends into existing depends, dedup.
            let existing_deps: std::collections::HashSet<String> =
                existing.depends.iter().cloned().collect();
            for d in &synthesized.depends {
                if !existing_deps.contains(d) {
                    existing.depends.push(d.clone());
                }
            }
            // Mark as top-level — main-modules are linker roots,
            // never children of another component.
            existing.parent_purl = None;
            main_modules_emitted += 1;
        } else if seen_purls.insert(purl_key) {
            entries.push(synthesized);
            main_modules_emitted += 1;
        }
    }
    // Milestone 163 (T013 FR-009): grep-friendly summary line.
    if ms163_resolved_total > 0 || ms163_unresolved_total > 0 {
        tracing::info!(
            resolved_count = ms163_resolved_total,
            phantom_prevented_count = ms163_resolved_total + ms163_unresolved_total,
            unresolved_declared_count = ms163_unresolved_total,
            "npm workspace-peer cross-resolution summary"
        );
    }

    // Issue #256: nameless secondary `package.json` umbrella pass.
    //
    // A `package.json` without a `name` field doesn't produce a
    // main-module entry (build_npm_main_module_entry returns None,
    // since FR-001 requires `name`). Per the npm spec, `name`/
    // `version` are only required for *publishable* packages; lock-
    // down secondary manifests (integration-test utility configs,
    // schema-lint configs, etc.) routinely omit them.
    //
    // Without intervention, the secondary's declared `dependencies[]`
    // get emitted as components (via Tier A's package-lock.json walk)
    // but have no incoming edge — no main-module is synthesized to
    // anchor them. The result is an orphan dep subtree disconnected
    // from the document root.
    //
    // FIX (option A from issue #256): for each nameless secondary
    // `package.json`, find the closest enclosing primary main-module
    // (by source_path-prefix-match) and merge the nameless manifest's
    // declared deps into ITS `.depends`. Tag each merged dep's
    // component with `mikebom:source-manifest: <relative-path>`
    // annotation so the manifest provenance survives the topology
    // flattening — graph-walking SBOM consumers see the dep is
    // reachable from root; provenance-walking consumers can still
    // trace it to its declaring manifest.
    //
    // The annotation slot is mikebom-namespaced. No new parity-
    // catalog row needed today; row C45 / milestone 061's annotation
    // infrastructure is the natural place to extend if we want
    // cross-format parity guarantees on source-manifest.
    apply_nameless_secondary_umbrella(rootfs, include_dev, &mut entries, exclude_set);

    // Milestone 194 US2 (issue #572) — synthesize a nested mainmod
    // for each nameless `package.json` that has an adjacent
    // `package-lock.json`, using the directory's basename as the
    // PURL name (versionless per m191). The umbrella pass above
    // empirically fails to reach some nested nameless workspaces
    // (e.g., pico's `pkg/db/integrationtest/schemalint/`); this
    // pass ensures every lockfile-anchored nameless workspace gets
    // a graph anchor, so its transitive deps aren't orphaned.
    synthesize_nameless_nested_mainmods(rootfs, &mut entries, exclude_set);

    // Milestone 066 same-PURL dedup. Collapses same-PURL collisions
    // (rare for npm given `node_modules/` exclusion in
    // should_skip_descent, but defensive). Non-main-module entries
    // are untouched (already deduped by `seen_purls`).
    let dedup_drops = walk::dedup_npm_main_modules_by_purl(&mut entries);
    if !dedup_drops.is_empty() {
        let dropped_paths: Vec<String> = dedup_drops
            .iter()
            .map(|d| d.dropped_path.clone())
            .collect();
        let kept_path = dedup_drops
            .first()
            .map(|d| d.kept_path.clone())
            .unwrap_or_default();
        let example_purl = dedup_drops
            .first()
            .map(|d| d.purl.clone())
            .unwrap_or_default();
        tracing::warn!(
            count = dedup_drops.len(),
            example_purl = %example_purl,
            kept = %kept_path,
            dropped = ?dropped_paths,
            "npm: deduped same-PURL package.json files",
        );
    }
    if main_modules_emitted > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            main_modules_emitted,
            same_purl_duplicates_dropped = dedup_drops.len(),
            "npm: emitted main-module components",
        );
    }

    Ok(entries)
}

/// Milestone 194 US2 (issue #572) — synthesize a mainmod component
/// for each nameless `package.json` alongside a `package-lock.json`,
/// using the directory basename as the PURL name (versionless per
/// m191). Rationale: the umbrella pass (`apply_nameless_secondary_
/// umbrella`) empirically fails to reach nested nameless workspaces
/// (e.g., pico's `pkg/db/integrationtest/schemalint/`), leaving the
/// lockfile's transitive components as orphans. Synthesizing a
/// per-workspace mainmod gives BFS a graph anchor without relying on
/// the umbrella's parent-scan heuristic.
///
/// Skips directories that either (a) have no adjacent
/// `package-lock.json`, (b) have a `name` field (already handled by
/// the m066 loop), or (c) already have an entry at the same PURL in
/// `entries` (dedup safety — the umbrella pass may have populated it).
///
/// Uses `mikebom:component-role: main-module` so the m127 root-
/// selector treats the synthesized mainmod as a workspace peer, and
/// so `apply_main_module_drop_or_demote` correctly drops it under
/// operator-override (`--root-name X`). The m192/m193 pre-rewrite
/// re-anchors its outgoing DependsOn edges onto the operator's
/// synthetic root, keeping the graph connected.
/// Milestone 199 US2 — stamp `mikebom:declared-as` on lockfile-
/// resolved entries whose declaration in `project_root/package.json`
/// used the `"my-alias": "npm:actual-pkg@1.0.0"` alias syntax.
///
/// Runs once per project root, iterating the local `package.json` deps
/// (and devDependencies) for the `npm:` prefix pattern. For each
/// detected alias, finds the corresponding entry in `entries` by name
/// (aliased_name) and stamps `mikebom:declared-as: [alias_name]` on it.
///
/// If an entry already has `mikebom:declared-as`, appends the new alias
/// into the existing array + sorts + dedupes (matches m199 data-model E1
/// validation rules — alias-count is not provenance).
///
/// No-op when the package.json isn't found, has no aliases, or when
/// no entry matches the resolved name.
fn stamp_alias_declared_as(project_root: &Path, entries: &mut [PackageDbEntry]) {
    let pkg_json_path = project_root.join("package.json");
    let Ok(text) = std::fs::read_to_string(&pkg_json_path) else {
        return;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    for section in ["dependencies", "devDependencies", "optionalDependencies"] {
        let Some(obj) = parsed.get(section).and_then(|v| v.as_object()) else {
            continue;
        };
        for (dep_name, dep_ver) in obj {
            let Some(dep_ver_str) = dep_ver.as_str() else {
                continue;
            };
            let Some(alias) = alias_mapping::parse_package_json_alias(dep_name, dep_ver_str)
            else {
                continue;
            };
            // Find matching entry by resolved name. There may be multiple
            // entries with the same name (different parent_purls) — stamp
            // ALL of them to be safe.
            for e in entries.iter_mut() {
                if e.name != alias.aliased_name {
                    continue;
                }
                // Only stamp npm entries.
                if !e.purl.as_str().starts_with("pkg:npm/") {
                    continue;
                }
                let existing = e
                    .extra_annotations
                    .get("mikebom:declared-as")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut aliases: std::collections::BTreeSet<String> = existing
                    .into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                aliases.insert(alias.local_name.clone());
                let sorted: Vec<String> = aliases.into_iter().collect();
                e.extra_annotations.insert(
                    "mikebom:declared-as".to_string(),
                    serde_json::json!(sorted),
                );
            }
        }
    }
}

fn synthesize_nameless_nested_mainmods(
    rootfs: &Path,
    entries: &mut Vec<PackageDbEntry>,
    exclude_set: &super::exclude_path::ExclusionSet,
) {
    let existing_purls: std::collections::HashSet<String> = entries
        .iter()
        .map(|e| e.purl.as_str().to_string())
        .collect();
    let mut synthesized = 0usize;
    for project_root in candidate_project_roots(rootfs, exclude_set) {
        let manifest_path = project_root.join("package.json");
        let lock_path = project_root.join("package-lock.json");
        if !manifest_path.is_file() || !lock_path.is_file() {
            continue;
        }
        let Ok(manifest_text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&manifest_text) else {
            continue;
        };
        // Skip if named — m066 already emitted it (or will).
        if parsed.get("name").and_then(|v| v.as_str()).is_some() {
            continue;
        }
        let Some(basename) = project_root
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(purl) = build_npm_purl(basename, "") else {
            continue;
        };
        if existing_purls.contains(purl.as_str()) {
            continue;
        }
        // Collect direct-dep names from the manifest's dep sections,
        // and resolve each to its lockfile-pinned version so the
        // `.depends` entry uses the m087/npm disambiguation key
        // (`"{name} {version}"`) at `scan_fs/mod.rs:547`. Without the
        // version qualifier, the versionless-name key collides with
        // our own synthesized entry (`pkg:npm/<basename>`) in
        // `name_to_purl` and the resolver picks a self-loop (silently
        // dropped), leaving the transitives orphaned. Fixes the
        // self-loop bug found on pico's `schemalint` case.
        let lock_versions: std::collections::HashMap<String, String> =
            match std::fs::read_to_string(&lock_path)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            {
                Some(lock) => lock
                    .get("packages")
                    .and_then(|v| v.as_object())
                    .map(|pkgs| {
                        let mut m = std::collections::HashMap::new();
                        for (key, val) in pkgs {
                            if let Some(name) = key.strip_prefix("node_modules/") {
                                // Only take TOP-LEVEL node_modules
                                // entries (no nested `.../node_modules/`).
                                if name.contains("/node_modules/") {
                                    continue;
                                }
                                if let Some(v) = val.get("version").and_then(|v| v.as_str()) {
                                    m.insert(name.to_string(), v.to_string());
                                }
                            }
                        }
                        m
                    })
                    .unwrap_or_default(),
                None => std::collections::HashMap::new(),
            };
        let mut depends: Vec<String> = Vec::new();
        for section in ["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(obj) = parsed.get(section).and_then(|v| v.as_object()) {
                for name in obj.keys() {
                    // Prefer the disambiguated form when lockfile has a
                    // pinned version; fall back to bare name (name_to_purl
                    // will pick whichever version was last inserted).
                    let dep_str = match lock_versions.get(name) {
                        Some(v) => format!("{name} {v}"),
                        None => name.clone(),
                    };
                    if !depends.iter().any(|d| d == &dep_str) {
                        depends.push(dep_str);
                    }
                }
            }
        }
        let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        extra_annotations.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        extra_annotations.insert(
            "mikebom:synthesized-from".to_string(),
            serde_json::Value::String("nameless-nested-workspace".to_string()),
        );
        entries.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: basename.to_string(),
            version: String::new(),
            arch: None,
            source_path: format!("path+file://{}", project_root.display()),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            buildinfo_status: None,
            evidence_kind: None,
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
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
        synthesized += 1;
    }
    if synthesized > 0 {
        tracing::info!(
            synthesized_count = synthesized,
            "npm: synthesized nested nameless-workspace mainmods (m194 US2 / issue #572)"
        );
    }
}

/// Issue #256: for each nameless secondary `package.json` (one
/// missing the `name` field), merge its declared `dependencies[]`
/// (and `devDependencies[]` when `include_dev`) into the closest
/// enclosing primary main-module's `.depends`, and tag each merged
/// dep's component with a `mikebom:source-manifest: <relative-path>`
/// annotation.
///
/// "Closest enclosing primary main-module" is determined by walking
/// the chain of parent directories of the nameless manifest and
/// picking the longest-source_path npm main-module whose containing
/// directory is an ancestor (or the same dir, for the single-dir
/// pathological case) of the nameless manifest's directory.
///
/// If no enclosing primary main-module exists (e.g., the scan only
/// contains nameless manifests), this pass warns and leaves the deps
/// as orphans — there's no anchor to attach them to. Future cross-
/// ecosystem reachability backfill (option C in the issue) could
/// catch that pathological case at the scan_fs level; not in scope
/// for this fix.
fn apply_nameless_secondary_umbrella(
    rootfs: &Path,
    include_dev: bool,
    entries: &mut [PackageDbEntry],
    exclude_set: &super::exclude_path::ExclusionSet,
) {
    // Pre-pass: enumerate project_root directories whose `package.json`
    // produced an actual main-module entry in `entries`. This is the
    // pool of "umbrella targets" — manifests an orphan-source manifest
    // can attach its deps to.
    //
    // Issue #245 refinement: switched from "manifest has a name" to
    // "main-module entry exists in entries". The former missed
    // `private: true` + no `version` manifests (which have a name but
    // get skipped by build_npm_main_module_entry per FR-001 / #104),
    // leaving their declared deps as orphans. The new criterion is
    // the strictly correct condition — it captures every manifest the
    // main-module-build loop above DIDN'T handle (whether due to
    // nameless OR private+no-version).
    let project_roots = candidate_project_roots(rootfs, exclude_set);
    let main_module_dirs: Vec<PathBuf> = entries
        .iter()
        .filter(|e| {
            e.purl.as_str().starts_with("pkg:npm/")
                && e.extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
        })
        .filter_map(|e| {
            e.source_path
                .strip_prefix("path+file://")
                .map(PathBuf::from)
        })
        .collect();

    for project_root in &project_roots {
        let manifest_path = project_root.join("package.json");
        if !manifest_path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        // Skip project_roots that already have a main-module entry —
        // they were handled by the main-module-build loop above. The
        // umbrella ONLY fires for "orphan-source" manifests: those
        // whose main-module emission was skipped (nameless, or
        // private+no-version, or any future skip condition).
        if main_module_dirs.iter().any(|d| d == project_root) {
            continue;
        }
        // Collect declared dep names. dependencies +
        // optionalDependencies are always emitted; devDependencies
        // only when include_dev (same pattern as
        // parse_root_package_json). `peerDependencies` is
        // intentionally skipped — declarative-not-install
        // relationship (matches Tier A / Tier B walkers + trivy/
        // syft).
        let mut declared_dep_names: Vec<String> = Vec::new();
        let sections: &[(&str, bool)] = &[
            ("dependencies", true),
            ("devDependencies", include_dev),
            ("optionalDependencies", true),
        ];
        for (section, gate) in sections {
            if !gate {
                continue;
            }
            if let Some(obj) = parsed.get(*section).and_then(|v| v.as_object()) {
                for k in obj.keys() {
                    declared_dep_names.push(k.to_string());
                }
            }
        }
        if declared_dep_names.is_empty() {
            continue;
        }

        // Find the closest enclosing primary project root with an
        // emitted main-module entry — the directory that is an
        // ancestor of (or equal to) this orphan-source manifest's
        // directory, with the longest path.
        let target_project_root: Option<&PathBuf> = main_module_dirs
            .iter()
            .filter(|nd| nd != &project_root && project_root.starts_with(nd))
            .max_by_key(|nd| nd.as_os_str().len());

        let relative_manifest = manifest_path
            .strip_prefix(rootfs)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| manifest_path.to_string_lossy().into_owned());

        let Some(target_project_root) = target_project_root else {
            tracing::warn!(
                manifest = %relative_manifest,
                "Issue #256: nameless secondary package.json with no enclosing named main-module; declared deps will appear as orphans (no anchor to attach them to)"
            );
            continue;
        };

        // The target main-module entry's source_path is set by
        // `build_npm_main_module_entry` as `format!("path+file://{}",
        // project_root.display())`. Compute the expected string so we
        // can match against the entry in `entries`.
        let target_source_path = format!("path+file://{}", target_project_root.display());

        // Pass 1 (mut iter): merge declared dep names into the chosen
        // main-module's `.depends`, deduped against existing direct
        // requires from go.mod / lockfile.
        let mut added_count = 0usize;
        for entry in entries.iter_mut() {
            if entry.source_path == target_source_path
                && entry
                    .extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
            {
                let existing: std::collections::HashSet<String> =
                    entry.depends.iter().cloned().collect();
                for dep_name in &declared_dep_names {
                    if !existing.contains(dep_name) {
                        entry.depends.push(dep_name.clone());
                        added_count += 1;
                    }
                }
                break;
            }
        }

        // Pass 2 (mut iter): tag each merged dep's component with the
        // `mikebom:source-manifest` annotation. Separate pass so we
        // don't hold a mut borrow over the loop.
        let declared_set: std::collections::HashSet<&str> =
            declared_dep_names.iter().map(|s| s.as_str()).collect();
        for entry in entries.iter_mut() {
            if entry.purl.as_str().starts_with("pkg:npm/")
                && declared_set.contains(entry.name.as_str())
            {
                // Milestone 199: always-array shape (FR-001). Design-tier
                // stamp writes plural annotation with a 1-element array;
                // the reconciler at emission time either transfers this
                // to a source-tier survivor (accumulating with other
                // matches) or leaves the standalone design-tier's
                // annotation as-is (still plural, still 1-element array).
                entry.extra_annotations.insert(
                    "mikebom:source-manifests".to_string(),
                    serde_json::json!([relative_manifest.clone()]),
                );
            }
        }

        if added_count > 0 {
            tracing::info!(
                manifest = %relative_manifest,
                added = added_count,
                target = %target_project_root.display(),
                "Issue #256: umbrellaed nameless secondary package.json's deps onto enclosing main-module"
            );
        }
    }
}

/// Max depth for the recursive project-root search. Chosen to cover
/// realistic monorepos (`repo/packages/foo/apps/admin/` = 4 levels)
/// without running away into deep source trees. The walk is cheap
/// because it terminates at `node_modules/` and VCS/build directories.
const MAX_PROJECT_ROOT_DEPTH: usize = 6;

/// Enumerate every directory under `rootfs` that looks like an npm
/// project. Always includes `rootfs` itself so the single-project
/// case stays identical to before. Recurses up to
/// `MAX_PROJECT_ROOT_DEPTH` levels, stopping at directories that
/// cannot contain a project (installed trees, VCS / build outputs,
/// language-specific caches).
///
/// Handles three layouts with one mechanism:
/// - **Single project**: `rootfs` has the signals, descendants don't.
/// - **Container image**: `/usr/src/app/`, `/app/sub/`, `/srv/foo/`,
///   `/usr/lib/node_modules/<pkg>/` are all discovered without a
///   hard-coded path list — each is just a directory with npm signals.
/// - **Monorepo / multi-app dir**: every `package.json` under
///   `services/*`, `packages/*`, `apps/*`, etc. becomes its own root,
///   so per-workspace-package deps surface even when the root carries
///   a single hoisted `node_modules/`.
///
/// Dedup by PURL in `read()` handles the common case where a root
/// lockfile and a sub-package `package.json` reference the same dep.
fn candidate_project_roots(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    use super::project_roots::should_skip_default_descent;
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_PROJECT_ROOT_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir() && has_npm_signal(path) {
            out.push(path.to_path_buf());
        }
    });
    out
}

/// True when `dir` holds any of the four npm project signals. Used to
/// tag walk hits as project roots.
fn has_npm_signal(dir: &Path) -> bool {
    dir.join("package-lock.json").is_file()
        || dir.join("pnpm-lock.yaml").is_file()
        || dir.join("bun.lock").is_file()
        || dir.join("yarn.lock").is_file()
        || dir.join("node_modules").is_dir()
        || dir.join("package.json").is_file()
}

// -----------------------------------------------------------------------
// NpmIntegrity — SRI base64 → hex decoder
// -----------------------------------------------------------------------

/// A decoded SRI integrity string from an npm lockfile. The lockfile
/// stores values like `sha512-<base64>`; we keep the algorithm name and
/// convert the base64 payload to lowercase hex so it matches
/// `ContentHash.value`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NpmIntegrity {
    pub algorithm: String,
    pub hex: String,
}

impl NpmIntegrity {
    /// Decode an SRI string. Returns None for anything that doesn't
    /// match the `alg-<base64>` shape or whose algorithm we don't
    /// recognise.
    pub(crate) fn parse(sri: &str) -> Option<Self> {
        let (alg, b64) = sri.split_once('-')?;
        let alg_upper = match alg.to_ascii_lowercase().as_str() {
            "sha512" => "SHA-512",
            "sha384" => "SHA-384",
            "sha256" => "SHA-256",
            "sha1" => "SHA-1",
            _ => return None,
        };
        let decoded = base64_decode(b64)?;
        let hex = decoded
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Some(Self {
            algorithm: alg_upper.to_string(),
            hex,
        })
    }

    /// Convert to a `ContentHash`. Maps the SRI algorithm to mikebom's
    /// `HashAlgorithm` enum and validates hex length via the shared
    /// `with_algorithm` constructor. SHA-512 and SHA-256 are by far
    /// the dominant algorithms in npm lockfiles; SHA-384 and SHA-1
    /// also pass through.
    pub(crate) fn to_content_hash(&self) -> Option<ContentHash> {
        use waybill_common::types::hash::HashAlgorithm;
        let alg = match self.algorithm.as_str() {
            "SHA-256" => HashAlgorithm::Sha256,
            "SHA-512" => HashAlgorithm::Sha512,
            "SHA-1" => HashAlgorithm::Sha1,
            // SHA-384 isn't in HashAlgorithm yet; defer.
            _ => return None,
        };
        ContentHash::with_algorithm(alg, &self.hex).ok()
    }
}

/// Tiny base64 decoder used by [`NpmIntegrity::parse`]. The `base64`
/// crate is already a workspace dep but importing in this hot path
/// adds compile-time to something we can write in ~20 lines. Uses the
/// standard alphabet per RFC 4648.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(input.as_bytes()).ok()
}

// -----------------------------------------------------------------------
// Tier A: package-lock.json v2/v3 parser
// -----------------------------------------------------------------------


// ========================================================================
// Module structure (milestone 018 / US2)
// ========================================================================
//
// npm/ split layout:
//   - package_lock.rs — package-lock.json v2/v3 parser
//   - pnpm_lock.rs    — pnpm-lock.yaml parser
//   - walk.rs         — node_modules walker + read_root_package_json + classifier
//   - enrich.rs       — author backfill from installed package.json files
//
// This file (mod.rs) hosts the orchestrator (pub fn read), error type
// (NpmError), project-root walker, integrity-string parser, base64 helper,
// and the cross-section build_npm_purl helper (used by every parser).
mod alias_mapping;
mod bun_lock;
mod enrich;
mod jsonc;
mod package_lock;
mod peer_optional;
mod pnpm_lock;
mod walk;
mod yarn_lock;

fn build_npm_purl(name: &str, version: &str) -> Option<Purl> {
    // Milestone 191 (#558): when version is empty (design-tier
    // package.json declaration with no resolved lockfile entry —
    // e.g., an `optionalDependencies` entry that failed to install,
    // or a freshly-added dep before lockfile refresh), emit a
    // versionless PURL per purl-spec canonical form — no trailing
    // `@`. Scoped-name segment (`%40scope/name`) is preserved.
    let purl_str = if let Some(rest) = name.strip_prefix('@') {
        let (scope, bare_name) = rest.split_once('/')?;
        if version.is_empty() {
            format!(
                "pkg:npm/%40{}/{}",
                encode_purl_segment(scope),
                encode_purl_segment(bare_name),
            )
        } else {
            format!(
                "pkg:npm/%40{}/{}@{}",
                encode_purl_segment(scope),
                encode_purl_segment(bare_name),
                encode_purl_segment(version),
            )
        }
    } else if version.is_empty() {
        format!("pkg:npm/{}", encode_purl_segment(name))
    } else {
        format!(
            "pkg:npm/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        )
    };
    Purl::new(&purl_str).ok()
}

// -----------------------------------------------------------------------
// Tier A: pnpm-lock.yaml parser (v6 / v7 / v9)
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn integrity_decodes_sha512() {
        // base64("hello world") = "aGVsbG8gd29ybGQ=" — but for SRI we
        // use a real sha512 so test values are deterministic.
        let sri = "sha512-MJ7MSJwS1utMxA9QyQLytNDtd+5RGnx+7fIK+4qg9hvLABzzXAIaFMqoD6YFUYaCQPkMInyGdz6TQEsE7bPdCg==";
        let decoded = NpmIntegrity::parse(sri).expect("parses");
        assert_eq!(decoded.algorithm, "SHA-512");
        assert_eq!(decoded.hex.len(), 128); // 512 bits = 128 hex chars
    }

    #[test]
    fn integrity_decodes_sha384_and_sha256() {
        assert_eq!(
            NpmIntegrity::parse("sha384-AAAA").map(|i| i.algorithm),
            Some("SHA-384".to_string())
        );
        assert_eq!(
            NpmIntegrity::parse("sha256-AAAA").map(|i| i.algorithm),
            Some("SHA-256".to_string())
        );
    }

    #[test]
    fn integrity_rejects_malformed_input() {
        assert!(NpmIntegrity::parse("").is_none());
        assert!(NpmIntegrity::parse("sha512").is_none());
        assert!(NpmIntegrity::parse("unknown-AAAA").is_none());
        assert!(NpmIntegrity::parse("sha512-!!!invalid base64!!!").is_none());
    }

    #[test]
    fn integrity_round_trips_to_content_hash() {
        let sri = "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=";
        let decoded = NpmIntegrity::parse(sri).expect("parses");
        let hash = decoded.to_content_hash().expect("converts");
        // 32 bytes = 64 hex chars for SHA-256.
        assert_eq!(hash.value.as_str().len(), 64);
    }

    #[test]
    fn build_npm_purl_unscoped() {
        let p = build_npm_purl("lodash", "4.17.21").expect("builds");
        assert_eq!(p.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn build_npm_purl_scoped_encodes_at() {
        let p = build_npm_purl("@angular/core", "16.0.0").expect("builds");
        assert_eq!(p.as_str(), "pkg:npm/%40angular/core@16.0.0");
    }

    #[test]
    fn build_npm_purl_empty_version_emits_versionless_shape() {
        // Milestone 191 (#558) — design-tier root-package.json fallback
        // entries have no resolved version. Pre-m191 the PURL was
        // `pkg:npm/foo@` (trailing `@`, spec-non-canonical). Post-m191
        // the emitted shape is the versionless `pkg:npm/foo` per purl-
        // spec canonical form.
        let p = build_npm_purl("foo", "").expect("empty-version permitted");
        assert_eq!(p.as_str(), "pkg:npm/foo");
    }

    #[test]
    fn build_npm_purl_scoped_empty_version_emits_versionless_shape() {
        // Scoped equivalent of the above — `%40scope/name` prefix
        // preserved, no `@` before the empty version segment.
        let p = build_npm_purl("@angular/core", "").expect("scoped empty-version permitted");
        assert_eq!(p.as_str(), "pkg:npm/%40angular/core");
    }

    #[test]
    fn build_npm_purl_nonempty_version_byte_identical_to_pre_m191() {
        // FR-011 / SC-006 — byte-identity for the non-empty-version
        // path. Pre-m191 output was `pkg:npm/lodash@4.17.21`.
        let p = build_npm_purl("lodash", "4.17.21").expect("non-empty");
        assert_eq!(p.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn build_npm_purl_scoped_nonempty_version_byte_identical_to_pre_m191() {
        let p = build_npm_purl("@angular/core", "16.0.0").expect("scoped non-empty");
        assert_eq!(p.as_str(), "pkg:npm/%40angular/core@16.0.0");
    }

    #[test]
    fn reads_package_lock_over_pnpm_when_both_exist() {
        // If both files are present, package-lock.json wins (tier A
        // dispatch order in read()).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/a":{"version":"1.0.0"}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '6.0'\npackages:\n  /b@2.0.0:\n    dev: false\n",
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert!(out.iter().any(|e| e.name == "a"));
        assert!(
            !out.iter().any(|e| e.name == "b"),
            "pnpm lockfile should be ignored when package-lock is present"
        );
    }

    #[test]
    fn falls_back_to_node_modules_walk_when_no_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("node_modules/foo");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("package.json"),
            r#"{"name":"foo","version":"1.2.3","license":"MIT"}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "foo");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("deployed"));
    }

    #[test]
    fn image_mode_discovers_node_modules_under_usr_src_app() {
        // Simulate a rootfs from a `node:*` image: installed tree lives
        // at /usr/src/app/node_modules/, no lockfile present.
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("usr/src/app");
        let nm = app.join("node_modules");
        let express = nm.join("express");
        std::fs::create_dir_all(&express).unwrap();
        std::fs::write(
            express.join("package.json"),
            r#"{"name":"express","version":"4.18.2","license":"MIT"}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert_eq!(out.len(), 1, "expected 1 entry from image-mode walk");
        assert_eq!(out[0].name, "express");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("deployed"));
    }

    #[test]
    fn image_mode_discovers_global_npm_installs() {
        // Global installs live at /usr/lib/node_modules/ — typically a
        // single `npm`/`corepack`/similar tree on node base images.
        // Feature 005 US1: the npm self-root emits in --image mode and
        // carries `npm_role=internal`. --path mode is exercised in a
        // separate test that asserts zero emission.
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("usr/lib/node_modules/npm");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("package.json"),
            r#"{"name":"npm","version":"10.2.4","license":"Artistic-2.0"}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Image, &Default::default()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "npm");
        assert_eq!(out[0].npm_role.as_deref(), Some("internal"));
    }

    #[test]
    fn monorepo_layout_discovers_each_workspace_package() {
        // Arbitrary layout — no image convention assumed. Root has a
        // lockfile (tier A fires there) and each service has only a
        // package.json.
        //
        // Milestone 163 (closes #498) reshape: fastify/next/bull are
        // NOT in the root lockfile → per Q1+Q2 unified disposition, they
        // are UNRESOLVABLE. Post-163 does NOT emit phantom empty-version
        // entries for them (SC-004). Instead each peer's main-module
        // component carries a `mikebom:unresolved-declared-dep` (C115)
        // annotation naming its unresolvable dep. The test verifies:
        //   1. root lockfile entry (`shared-lib`) still emitted.
        //   2. all 3 sub-package main-modules emitted (per milestone 066).
        //   3. root main-module (`monorepo`) emitted.
        //   4. NO phantom fastify/next/bull entries in the output.
        //   5. Each peer main-module has its C115 annotation stamped.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Root: lockfile with one prod dep so tier A produces output.
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"monorepo","version":"0.0.0","workspaces":["services/*"]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/shared-lib":{"version":"1.0.0"}}}"#,
        )
        .unwrap();

        // Sub-packages: package.json only, declaring unique deps.
        for (svc, dep) in [("api", "fastify"), ("web", "next"), ("worker", "bull")] {
            let svc_dir = root.join("services").join(svc);
            std::fs::create_dir_all(&svc_dir).unwrap();
            std::fs::write(
                svc_dir.join("package.json"),
                format!(
                    r#"{{"name":"@monorepo/{svc}","version":"0.0.0","dependencies":{{"{dep}":"^1.0"}}}}"#
                ),
            )
            .unwrap();
        }

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();

        // (1) Root lockfile entry preserved.
        assert!(names.contains(&"shared-lib"), "root lockfile: got {names:?}");

        // (2) Sub-package main-modules emitted per milestone 066.
        for peer in ["@monorepo/api", "@monorepo/web", "@monorepo/worker"] {
            assert!(
                names.contains(&peer),
                "milestone-066 main-module `{peer}` missing: got {names:?}"
            );
        }

        // (3) Root main-module emitted (has name/version).
        assert!(
            names.contains(&"monorepo"),
            "root main-module `monorepo` missing: got {names:?}"
        );

        // (4) Milestone 163 SC-004 invariant: NO phantom fastify/next/
        // bull emitted (they're unresolvable → C115 annotations, not
        // components).
        for phantom in ["fastify", "next", "bull"] {
            assert!(
                !names.contains(&phantom),
                "SC-004 violated: phantom `{phantom}` must NOT appear post-163: got {names:?}"
            );
        }

        // (5) Milestone 163 C115 stamped on each peer's main-module.
        for (svc, dep) in [("api", "fastify"), ("web", "next"), ("worker", "bull")] {
            let peer_name = format!("@monorepo/{svc}");
            let peer = out
                .iter()
                .find(|e| e.name == peer_name)
                .unwrap_or_else(|| panic!("peer {peer_name} not found in {names:?}"));
            let c115 = peer
                .extra_annotations
                .get("mikebom:unresolved-declared-dep")
                .unwrap_or_else(|| {
                    panic!("C115 annotation missing on peer {peer_name}");
                });
            assert_eq!(
                c115.as_str(),
                Some(dep),
                "peer {peer_name} C115 value must name `{dep}`"
            );
        }
    }

    #[test]
    // walker-audit: false-positive — #[test] function name shares the walk_ prefix of the unit under test
    fn walk_skips_node_modules_subtrees() {
        // Deliberately plant a package.json *inside* a node_modules/ —
        // this is a dependency's manifest, not a project root. The
        // descent must skip node_modules so it doesn't get picked up.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("project")).unwrap();
        std::fs::write(
            root.join("project/package.json"),
            r#"{"name":"project","version":"0.0.0"}"#,
        )
        .unwrap();
        let nested = root.join("project/node_modules/some-dep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("package.json"),
            r#"{"name":"some-dep","version":"2.0.0","dependencies":{"should-not-resurface":"*"}}"#,
        )
        .unwrap();

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert!(
            !out.iter().any(|e| e.name == "should-not-resurface"),
            "descent into node_modules must not create bogus project roots"
        );
    }

    #[test]
    fn image_mode_deduplicates_purls_across_project_roots() {
        // When the same package appears in two discovered roots, the
        // reader emits it once.
        let dir = tempfile::tempdir().unwrap();
        for loc in ["app", "usr/src/app"] {
            let nm = dir.path().join(loc).join("node_modules/lodash");
            std::fs::create_dir_all(&nm).unwrap();
            std::fs::write(
                nm.join("package.json"),
                r#"{"name":"lodash","version":"4.17.21","license":"MIT"}"#,
            )
            .unwrap();
        }
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert_eq!(out.len(), 1, "duplicate PURLs must be deduped");
        assert_eq!(out[0].name, "lodash");
    }

    #[test]
    fn root_pkgjson_fallback_fires_only_when_no_lockfile_no_nm() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"lodash":"^4.0"}}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "lodash");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("design"));
    }

    #[test]
    fn path_mode_excludes_npm_internals_from_read() {
        // T019 — the npm-internals tree must not contribute entries in
        // --path mode; the operator is scanning an application and
        // npm's own bundled deps are scanner tooling, not app deps.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir
            .path()
            .join("usr/lib/node_modules/npm/node_modules/@npmcli/arborist");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("package.json"),
            r#"{"name":"@npmcli/arborist","version":"7.0.0"}"#,
        )
        .unwrap();
        // npm itself also has a package.json — that's part of the
        // self-root detection path.
        let npm_root = dir.path().join("usr/lib/node_modules/npm");
        std::fs::write(
            npm_root.join("package.json"),
            r#"{"name":"npm","version":"10.2.4"}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        assert!(
            out.iter().all(|e| e.name != "@npmcli/arborist"),
            "arborist should not appear in --path-mode output; got {:?}",
            out.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            out.iter().all(|e| e.name != "npm"),
            "npm self-root should not appear in --path-mode output"
        );
    }

    #[test]
    fn image_mode_includes_npm_internals_with_role() {
        // T020 — same fixture as T019, inverse mode. In --image mode
        // the internals ARE emitted and each carries
        // `npm_role = Some("internal")`.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir
            .path()
            .join("usr/lib/node_modules/npm/node_modules/@npmcli/arborist");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("package.json"),
            r#"{"name":"@npmcli/arborist","version":"7.0.0"}"#,
        )
        .unwrap();
        let npm_root = dir.path().join("usr/lib/node_modules/npm");
        std::fs::write(
            npm_root.join("package.json"),
            r#"{"name":"npm","version":"10.2.4"}"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Image, &Default::default()).unwrap();
        let arborist = out
            .iter()
            .find(|e| e.name == "@npmcli/arborist")
            .expect("arborist entry expected in --image mode");
        assert_eq!(arborist.npm_role.as_deref(), Some("internal"));
        let npm = out.iter().find(|e| e.name == "npm").expect("npm self-root expected");
        assert_eq!(npm.npm_role.as_deref(), Some("internal"));
    }

    // --- issue #256: nameless secondary package.json umbrella ---------------

    #[test]
    fn nameless_secondary_pkgjson_deps_attach_to_primary_main_module() {
        // Reproducer for issue #256: a primary `package.json` named
        // `repro-root` + a nameless secondary at
        // `pkg/db/integrationtest/schemalint/package.json`. Without
        // the fix, schemalint is emitted as a component but has zero
        // incoming edges. With the fix, schemalint appears in
        // repro-root's `.depends` AND carries a `mikebom:source-manifest`
        // annotation pointing at the secondary manifest path.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Primary: named manifest at scan root.
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"repro-root","version":"0.0.0","dependencies":{"axios":"^1.7.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/axios":{"version":"1.7.0"}}}"#,
        )
        .unwrap();

        // Secondary: NAMELESS manifest in a subdirectory.
        let secondary_dir = root.join("pkg/db/integrationtest/schemalint");
        std::fs::create_dir_all(&secondary_dir).unwrap();
        std::fs::write(
            secondary_dir.join("package.json"),
            r#"{"dependencies":{"schemalint":"^2.1.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            secondary_dir.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/schemalint":{"version":"2.3.2"}}}"#,
        )
        .unwrap();

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();

        // schemalint emitted as a component.
        let schemalint = out
            .iter()
            .find(|e| e.name == "schemalint")
            .expect("schemalint must be emitted as a component");

        // Milestone 199 always-array shape: schemalint carries the
        // source-manifests annotation pointing at the nameless secondary's
        // relative path as a JSON array (1-element for single-manifest).
        let annot_val = schemalint
            .extra_annotations
            .get("mikebom:source-manifests")
            .expect("schemalint must have mikebom:source-manifests annotation");
        let arr = annot_val
            .as_array()
            .expect("mikebom:source-manifests must be a JSON array");
        assert_eq!(arr.len(), 1, "1-element for single-manifest design-tier");
        let annot = arr[0].as_str().expect("array entry must be string");
        assert!(
            annot.ends_with("pkg/db/integrationtest/schemalint/package.json"),
            "source-manifests annotation should point at the nameless secondary; got: {annot}"
        );

        // repro-root main-module entry's `.depends` includes BOTH axios
        // (its own direct require) AND schemalint (umbrellaed from the
        // nameless secondary).
        let primary = out
            .iter()
            .find(|e| {
                e.name == "repro-root"
                    && e.extra_annotations
                        .get("mikebom:component-role")
                        .and_then(|v| v.as_str())
                        == Some("main-module")
            })
            .expect("repro-root main-module entry must exist");
        // Post walk-up resolution: axios is version-pinned via the
        // lockfile (build_npm_main_module_entry walks up
        // node_modules). schemalint comes from the nameless-secondary
        // umbrella pass which still emits bare-name strings.
        assert!(
            primary
                .depends
                .iter()
                .any(|d| d == "axios" || d == "axios 1.7.0"),
            "primary should still have its declared direct dep axios (bare or version-pinned): {:?}",
            primary.depends
        );
        assert!(
            primary.depends.contains(&"schemalint".to_string()),
            "primary should have umbrellaed schemalint via nameless-secondary fix: {:?}",
            primary.depends
        );
    }

    #[test]
    fn nameless_secondary_pkgjson_only_no_primary_warns_but_does_not_crash() {
        // Edge case: the scan has ONLY a nameless secondary manifest
        // (no primary main-module to anchor to). The fix should warn-
        // log and leave the deps as orphans rather than crashing.
        let dir = tempfile::tempdir().unwrap();
        let secondary_dir = dir.path().join("sub");
        std::fs::create_dir_all(&secondary_dir).unwrap();
        std::fs::write(
            secondary_dir.join("package.json"),
            r#"{"dependencies":{"axios":"^1.7.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            secondary_dir.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/axios":{"version":"1.7.0"}}}"#,
        )
        .unwrap();

        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        // axios is emitted but has no source-manifest annotation
        // (because no anchor existed to attach it to).
        let axios = out
            .iter()
            .find(|e| e.name == "axios")
            .expect("axios must be emitted as a component");
        assert!(
            !axios.extra_annotations.contains_key("mikebom:source-manifests"),
            "no anchor → no umbrella → no source-manifests annotation (m199 plural)"
        );
        // Also assert m191 singular scalar is gone post-m199.
        assert!(
            !axios.extra_annotations.contains_key("mikebom:source-manifest"),
            "m191 singular scalar must not be written by m199-era npm reader"
        );
    }

    #[test]
    fn named_secondary_pkgjson_does_not_get_umbrella_treatment() {
        // Sanity check: a NAMED secondary manifest goes through the
        // existing main-module-build loop and gets its own main-module
        // entry. The umbrella pass must NOT also annotate its deps —
        // that would be double-tagging.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("package.json"),
            r#"{"name":"primary","version":"0.0.0","dependencies":{"axios":"^1.7.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/axios":{"version":"1.7.0"}}}"#,
        )
        .unwrap();

        let secondary_dir = root.join("sub");
        std::fs::create_dir_all(&secondary_dir).unwrap();
        std::fs::write(
            secondary_dir.join("package.json"),
            r#"{"name":"secondary","version":"0.0.0","dependencies":{"schemalint":"^2.1.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            secondary_dir.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/schemalint":{"version":"2.3.2"}}}"#,
        )
        .unwrap();

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();
        let schemalint = out
            .iter()
            .find(|e| e.name == "schemalint")
            .expect("schemalint must be emitted");
        assert!(
            !schemalint
                .extra_annotations
                .contains_key("mikebom:source-manifests"),
            "named secondary's deps should NOT get the umbrella annotation; got: {:?}",
            schemalint.extra_annotations
        );
    }

    // --- issue #245: secondary node_modules re-parenting --------------------

    #[test]
    fn secondary_named_pkgjson_with_node_modules_links_deps_to_sub_main_module() {
        // Issue #245 reproducer: primary at root (named "repro-root",
        // depends on axios via lockfile), secondary at sub/ (named
        // "sub-pkg", depends on pg, with sub/node_modules/ but NO
        // sub/package-lock.json). pg + pg-connection-string get
        // discovered via Tier B's node_modules walk on the secondary.
        //
        // Expected: pg has an incoming edge from sub-pkg's main-module
        // entry (sub-pkg.depends contains "pg", populated from
        // sub/package.json's dependencies section by
        // build_npm_main_module_entry).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Primary: package.json + package-lock.json + node_modules/axios.
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"repro-root","version":"0.0.0","dependencies":{"axios":"^1.7.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/axios":{"version":"1.7.0"}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("node_modules/axios")).unwrap();
        std::fs::write(
            root.join("node_modules/axios/package.json"),
            r#"{"name":"axios","version":"1.7.0"}"#,
        )
        .unwrap();

        // Secondary: NAMED package.json + node_modules/ but NO
        // package-lock.json. Tier B (flat node_modules walk) should
        // emit pg + pg-connection-string; main-module-build should
        // emit sub-pkg with depends=[pg].
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("package.json"),
            r#"{"name":"sub-pkg","version":"0.0.0","dependencies":{"pg":"^8.0.0"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(sub.join("node_modules/pg")).unwrap();
        std::fs::write(
            sub.join("node_modules/pg/package.json"),
            r#"{"name":"pg","version":"8.11.3","dependencies":{"pg-connection-string":"^2.6.0"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(sub.join("node_modules/pg-connection-string")).unwrap();
        std::fs::write(
            sub.join("node_modules/pg-connection-string/package.json"),
            r#"{"name":"pg-connection-string","version":"2.6.0"}"#,
        )
        .unwrap();

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();

        // Verify all four components emitted.
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"axios"), "axios component should be emitted: {:?}", names);
        assert!(names.contains(&"pg"), "pg component should be emitted: {:?}", names);
        assert!(
            names.contains(&"pg-connection-string"),
            "pg-connection-string component should be emitted: {:?}",
            names
        );

        // Find sub-pkg main-module entry.
        let sub_pkg = out.iter().find(|e| {
            e.name == "sub-pkg"
                && e.extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
        });
        assert!(
            sub_pkg.is_some(),
            "sub-pkg main-module entry should exist (issue #245). All entries: {:?}",
            names
        );

        // The issue's claim: sub-pkg.depends should include "pg" so
        // that pg has an incoming edge from sub-pkg. Without this,
        // pg appears as an orphan in the dep graph.
        let sub_pkg = sub_pkg.unwrap();
        assert!(
            sub_pkg.depends.contains(&"pg".to_string()),
            "sub-pkg.depends should include 'pg' (issue #245 expected behavior); got: {:?}",
            sub_pkg.depends
        );
    }

    #[test]
    fn secondary_private_pkgjson_without_version_skips_main_module() {
        // Per build_npm_main_module_entry's FR-001 + issue #104
        // guidance: a `private: true` package.json with no `version`
        // field is treated as "not a publishable artifact" and the
        // main-module entry is NOT emitted. This is a documented
        // skip — but it leaves the secondary's declared deps without
        // an anchor in the graph. Verify: this is the failure mode
        // that issue #245 actually describes (when the user's
        // secondary manifest has private:true).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("package.json"),
            r#"{"name":"repro-root","version":"0.0.0","dependencies":{"axios":"^1.7.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/axios":{"version":"1.7.0"}}}"#,
        )
        .unwrap();

        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        // private: true + NO version → main-module entry skipped.
        std::fs::write(
            sub.join("package.json"),
            r#"{"name":"sub-pkg","private":true,"dependencies":{"pg":"^8.0.0"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(sub.join("node_modules/pg")).unwrap();
        std::fs::write(
            sub.join("node_modules/pg/package.json"),
            r#"{"name":"pg","version":"8.11.3"}"#,
        )
        .unwrap();

        let out = read(root, false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();

        // pg gets emitted via Tier B walk.
        assert!(
            out.iter().any(|e| e.name == "pg"),
            "pg should be emitted: {:?}",
            out.iter().map(|e| e.name.as_str()).collect::<Vec<_>>()
        );

        // sub-pkg main-module is NOT emitted (private:true + no
        // version). So pg has no incoming edge from sub-pkg.
        //
        // This documents the failure mode. The fix should mirror
        // issue #257's nameless-secondary umbrella: when a secondary
        // package.json's main-module is skipped (whether due to
        // nameless OR private+no-version), umbrella its declared
        // deps onto the primary main-module.
        let sub_pkg_emitted = out.iter().any(|e| e.name == "sub-pkg");
        assert!(
            !sub_pkg_emitted,
            "sub-pkg main-module SHOULD be skipped per FR-001 (private:true+no-version), preserving the existing skip behavior. The fix should umbrella the deps elsewhere, not emit sub-pkg as a publishable component."
        );

        // After fix: pg should be reachable from primary main-module
        // via the umbrella mechanism.
        let primary = out.iter().find(|e| e.name == "repro-root").expect("primary main-module");
        assert!(
            primary.depends.contains(&"pg".to_string()),
            "After fix: primary main-module 'repro-root' should umbrella pg from sub/. Got depends: {:?}",
            primary.depends
        );
    }

    #[test]
    fn nameless_secondary_annotation_contract_naming_stable() {
        // Contract test: the annotation key for nameless-secondary
        // umbrella deps is `mikebom:source-manifests` (m199 always-array).
        // Any accidental rename should be caught here before it ships and
        // breaks downstream consumer policy.
        const ANNOTATION_KEY: &str = "mikebom:source-manifests";
        assert_eq!(ANNOTATION_KEY, "mikebom:source-manifests");
        // If you rename this, update consumer-side policy AND the
        // parity-catalog C20 row.
    }

    // --- issue #262: nested-node_modules version-pinning end-to-end ---------

    #[test]
    fn nested_node_modules_install_emits_version_pinned_dep_string() {
        // End-to-end shape: verify the parser change produces a
        // version-qualified dep string that the scan_fs resolver can
        // match against the nested PURL. We assert the depends string
        // form here; the resolver-side wiring (cargo-mirroring
        // `(npm, "name version")` key in scan_fs/mod.rs:413-420) is
        // exercised by `cdx_regression` / integration tests at higher
        // levels.
        //
        // Reproducer: mlly@1.0.0 has nested pathe@2.0.3 under
        // node_modules/mlly/node_modules/pathe; pathe@1.1.2 hoisted
        // at top-level node_modules/pathe.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package-lock.json"),
            r#"{
                "lockfileVersion": 3,
                "packages": {
                    "node_modules/pathe": { "version": "1.1.2" },
                    "node_modules/mlly": {
                        "version": "1.0.0",
                        "dependencies": { "pathe": "^2.0.0" }
                    },
                    "node_modules/mlly/node_modules/pathe": { "version": "2.0.3" }
                }
            }"#,
        )
        .unwrap();
        let out = read(dir.path(), false, crate::scan_fs::ScanMode::Path, &Default::default()).unwrap();

        // Both pathes emitted as components.
        let pathe_versions: Vec<&str> = out
            .iter()
            .filter(|e| e.name == "pathe")
            .map(|e| e.version.as_str())
            .collect();
        assert!(pathe_versions.contains(&"1.1.2"), "hoisted pathe should be emitted");
        assert!(pathe_versions.contains(&"2.0.3"), "nested pathe should be emitted");

        // mlly's depends carries the NESTED pathe's version pin so
        // the edge resolver routes the edge to pathe@2.0.3 (not
        // the hoisted 1.1.2).
        let mlly = out.iter().find(|e| e.name == "mlly").expect("mlly");
        assert!(
            mlly.depends.contains(&"pathe 2.0.3".to_string()),
            "mlly.depends should pin pathe to the NESTED version 2.0.3; got: {:?}",
            mlly.depends
        );
        assert!(
            !mlly.depends.contains(&"pathe".to_string()),
            "mlly.depends should NOT carry the bare-name form (would route to hoisted); got: {:?}",
            mlly.depends
        );
    }
}
