//! pnpm-lock.yaml parser.
//!
//! Handles v6/v7 (single `packages:` section with inline
//! `dependencies` / `peerDependencies` / `optionalDependencies`)
//! AND v9 (`packages:` for identity + `snapshots:` for edges).
//! Milestone 157 (2026-07-03) added `snapshots:` support after the
//! team reported argo-cd's pnpm v9 lockfile emitting 1329 components
//! but only 110 dep-graph edges. Q1 clarification 2026-07-03 also
//! brought pnpm to parity with `package_lock.rs`'s milestone-147
//! behavior (walks all three non-dev dep sub-mappings — see
//! `PNPM_DEP_SECTIONS` const).

use std::path::Path;


use super::super::PackageDbEntry;
use super::{build_npm_purl, NpmIntegrity};

/// Milestone 157: pnpm dep-section names walked by both the snapshots
/// pre-scan (v9) AND the packages-inline path (v6/v7). Kept in one
/// place so the SC-011 pnpm/npm parity assertion has a stable code
/// anchor and so a future dep-section addition is a single edit.
///
/// NOT identical to `package_lock.rs`'s 4-section list — pnpm encodes
/// dev status via the per-package `dev: true` boolean at the entry
/// level (handled below in `parse_pnpm_lock`), NOT via a
/// `devDependencies:` sub-mapping. So `PNPM_DEP_SECTIONS` walks only
/// the three non-dev sections. The `dev: true` boolean continues to
/// gate whole-package filtering when `include_dev = false`.
const PNPM_DEP_SECTIONS: &[&str] = &[
    "dependencies",
    "peerDependencies",
    "optionalDependencies",
];

/// Milestone 157: walk the three dep sub-mappings inside a single
/// packages-entry table (v6/v7 inline path). Returns the sorted-
/// deduped union of the sub-mappings' KEYS (dep NAMES only — the
/// scan_fs dep-graph resolver at `scan_fs/mod.rs:700` keys
/// `name_to_purl` by `(ecosystem, name)`, matching every other npm
/// sub-reader's convention). Peer-dep suffixes on the KEY column
/// are irrelevant (dep-name column is a plain package name); we
/// only strip suffixes to filter out non-registry values in the
/// VERSION column (git URLs, tarballs, file paths) via
/// `parse_pnpm_key` on a synthesized `"<name>@<value>"` string.
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    // Milestone 164 (T003, implements m164 FR-001): when true, push
    // `"<name> <version>"` disambiguation form into `deps` so the
    // downstream `name_to_purl` lookup at `scan_fs/mod.rs:519-525`
    // (extended for npm per issue #262 + m087 cargo precedent) can
    // pick the correct version when multiple emitted `pkg:npm/<name>@*`
    // components exist. When false, preserve pre-164 bare-name
    // emission — used at the v6/v7 inline call site to satisfy m164
    // User Story 2 byte-identity guard.
    emit_versioned: bool,
    versioned_counter: Option<&mut usize>,
    warn_counter: Option<&mut usize>,
) -> Vec<String> {
    let mut deps: Vec<String> = Vec::new();
    let mut versioned_local: usize = 0;
    let mut warn_local: usize = 0;
    for section in PNPM_DEP_SECTIONS {
        let Some(sub) = entry_tbl
            .get(serde_yaml::Value::String((*section).to_string()))
            .and_then(|v| v.as_mapping())
        else {
            continue;
        };
        for (dep_key, dep_value) in sub {
            let Some(dep_name) = dep_key.as_str() else { continue };
            let Some(dep_ver_raw) = dep_value.as_str() else { continue };
            // Milestone 159: detect pnpm alias syntax on the VALUE
            // string. When the value's canonical name differs from
            // the local dep-key, emit an AliasResolution and use the
            // ALIASED name as the edge target (FR-003 + FR-005).
            if let Some(alias) =
                super::alias_mapping::detect_pnpm_alias(dep_name, dep_ver_raw, source_path)
            {
                // Milestone 164: when emit_versioned, carry the aliased
                // version through for the downstream `rewrite_dep_names`
                // pass to preserve on substitution (m164 FR-003 + R7).
                if emit_versioned && !alias.aliased_version.is_empty() {
                    deps.push(format!("{} {}", alias.aliased_name, alias.aliased_version));
                    versioned_local += 1;
                } else {
                    deps.push(alias.aliased_name.clone());
                }
                aliases.push(alias);
                continue;
            }
            // Non-alias path (or self-referential value). Validate the
            // VALUE is a registry-source string via parse_pnpm_key
            // round-trip; drop non-registry sources.
            let dep_pair_raw = format!("{dep_name}@{dep_ver_raw}");
            let stripped = dep_pair_raw
                .strip_prefix('/')
                .unwrap_or(&dep_pair_raw);
            let Some((canon_name, canon_ver)) = parse_pnpm_key(stripped) else {
                tracing::debug!(dep = %dep_pair_raw, "pnpm-lock: skipping non-registry dep value");
                continue;
            };
            // Milestone 164 (T003, FR-001 + FR-008): thread the version
            // through when the caller wants disambiguation.
            if emit_versioned {
                if canon_ver.is_empty() {
                    tracing::warn!(
                        key = %stripped,
                        "pnpm-lock v9: peer-dep-suffixed key parsed to empty version; falling back to bare-name form"
                    );
                    warn_local += 1;
                    deps.push(canon_name);
                } else {
                    versioned_local += 1;
                    deps.push(format!("{canon_name} {canon_ver}"));
                }
            } else {
                deps.push(canon_name);
            }
        }
    }
    deps.sort();
    deps.dedup();
    if let Some(c) = versioned_counter {
        *c += versioned_local;
    }
    if let Some(c) = warn_counter {
        *c += warn_local;
    }
    deps
}

/// Milestone 157: pre-scan the top-level `snapshots:` section
/// (introduced in pnpm-lock.yaml v9) into a lookup table keyed by
/// canonical `name@version` (peer-dep suffix stripped via
/// `parse_pnpm_key`). Values are the sorted-deduped union of the
/// three sub-mappings' keys, each normalized to canonical form.
///
/// Returns empty HashMap when the top-level `snapshots:` key is
/// missing or not a mapping (v6/v7 lockfiles, or anomalous v9
/// lockfiles).
fn build_snapshots_lookup(
    root: &serde_yaml::Value,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    // Milestone 164 (T004): threaded from `parse_pnpm_lock` for
    // the FR-009 info-log summary.
    versioned_counter: &mut usize,
    warn_counter: &mut usize,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut out = std::collections::HashMap::new();
    let Some(snapshots) = root
        .get("snapshots")
        .and_then(|v| v.as_mapping())
    else {
        return out;
    };
    for (key, entry) in snapshots {
        let Some(key_str) = key.as_str() else { continue };
        let stripped = key_str.strip_prefix('/').unwrap_or(key_str);
        let Some((name, version)) = parse_pnpm_key(stripped) else {
            tracing::debug!(snapshot_key = %key_str, "pnpm-lock: skipping non-registry snapshot key");
            continue;
        };
        let canonical = format!("{name}@{version}");
        let Some(tbl) = entry.as_mapping() else { continue };
        // Milestone 164 (T004, FR-001): v9 snapshots are the load-bearing
        // multi-version site — emit disambiguation form.
        let deps = collect_pnpm_dep_names(
            tbl,
            aliases,
            source_path,
            /* emit_versioned = */ true,
            Some(versioned_counter),
            Some(warn_counter),
        );
        out.insert(canonical, deps);
    }
    out
}

pub(super) fn read_pnpm_lock(rootfs: &Path, include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("pnpm-lock.yaml");
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&text).ok()?;
    let source_path = path.to_string_lossy().into_owned();
    let out = parse_pnpm_lock(&parsed, &source_path, include_dev);
    if out.is_empty() { None } else { Some(out) }
}

/// Parse a deserialised `pnpm-lock.yaml` document. Handles v6/v7/v9
/// dialects per research.md R5. Milestone 157: v9's `snapshots:`
/// section is now the authoritative source for per-package dep-graph
/// edges (v6/v7 continued to use inline `packages:` entries). Both
/// paths now walk the union of the three non-dev sub-mappings
/// (`dependencies`, `peerDependencies`, `optionalDependencies`) per
/// Q1 clarification 2026-07-03, matching milestone-147 npm parity.
pub(crate) fn parse_pnpm_lock(
    root: &serde_yaml::Value,
    source_path: &str,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let mut fell_back_count: usize = 0;
    // Milestone 164 (T006, FR-009): tally counters threaded through
    // `build_snapshots_lookup` and surfaced in the `pnpm-lock parsed`
    // info-log summary at the end of this function.
    let mut multi_version_disambiguated_count: usize = 0;
    let mut malformed_key_warn_count: usize = 0;

    // Milestone 157: lockfileVersion detection for FR-007 / FR-008
    // diagnostic gating. Field may be quoted string ('9.0') or
    // unquoted number (6.0); accept both.
    let lock_version: String = root
        .get("lockfileVersion")
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_f64().map(|n| n.to_string()))
        })
        .unwrap_or_default();
    let is_v9_or_later: bool = lock_version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|major| major >= 9)
        .unwrap_or(false);

    // Milestone 159 accumulator: alias-detection outputs from both
    // the snapshots first-pass AND the v6/v7 inline second-pass are
    // collected here for later edge-rewrite + annotation emission.
    let mut aliases: Vec<super::alias_mapping::AliasResolution> = Vec::new();

    // Milestone 157: pre-scan the v9 `snapshots:` section into a
    // lookup keyed by canonical name@version. Empty HashMap on
    // v6/v7 (no snapshots section) — the inline packages path
    // takes precedence via collect_pnpm_dep_names.
    let snapshots_lookup = build_snapshots_lookup(
        root,
        &mut aliases,
        source_path,
        &mut multi_version_disambiguated_count,
        &mut malformed_key_warn_count,
    );

    // v6/v7 put per-package info under `packages:` keyed by
    // "/<name>@<version>" (or "/@scope/name@version"). v9 removes
    // the leading slash and moves dep edges to `snapshots:`; the
    // packages: side becomes identity + integrity metadata.
    let Some(packages) = root.get("packages").and_then(|v| v.as_mapping()) else {
        return out;
    };

    let mut keys: Vec<String> = packages
        .keys()
        .filter_map(|k| k.as_str().map(|s| s.to_string()))
        .collect();
    keys.sort();

    for key in keys {
        let Some(entry) = packages.get(serde_yaml::Value::String(key.clone())) else {
            continue;
        };
        let Some(tbl) = entry.as_mapping() else { continue };

        // v6/v7 key form: "/foo@1.0.0" or "/@scope/name@1.0.0"
        // v9 key form: "foo@1.0.0" (no leading slash)
        let stripped = key.strip_prefix('/').unwrap_or(&key);
        let (name, version) = parse_pnpm_key(stripped).unwrap_or_default();
        if name.is_empty() || version.is_empty() {
            continue;
        }

        let is_dev = tbl
            .get(serde_yaml::Value::String("dev".to_string()))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Milestone 180 T012 — extract the per-entry `optional: true`
        // marker (pnpm sets this on entries reachable only through
        // optional edges; parallel to npm's `optional` flag). Currently
        // unused as a filter — the m179 `--include-dev` gating rides
        // through the m179 `is_non_runtime()` helper which applies to
        // any Optional-classified component.
        let is_optional = tbl
            .get(serde_yaml::Value::String("optional".to_string()))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Milestone 180 T012 — peer-precedence guard input. pnpm's
        // packages entries carry `peer: true` when the entry was
        // installed as a peer (parallel to npm's `peer` flag). Combined
        // with `optional: true` the entry is peer-optional; per FR-006
        // the m178 PROVIDED_DEPENDENCY_OF classification wins over
        // m180's OPTIONAL_DEPENDENCY_OF, so we short-circuit Optional
        // in the classifier below.
        let is_peer = tbl
            .get(serde_yaml::Value::String("peer".to_string()))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !include_dev && (is_dev || is_optional) {
            continue;
        }

        let Some(purl) = build_npm_purl(&name, &version) else {
            continue;
        };

        let hashes = tbl
            .get(serde_yaml::Value::String("resolution".to_string()))
            .and_then(|res| res.as_mapping())
            .and_then(|m| m.get(serde_yaml::Value::String("integrity".to_string())))
            .and_then(|v| v.as_str())
            .and_then(NpmIntegrity::parse)
            .and_then(|i| i.to_content_hash())
            .map(|h| vec![h])
            .unwrap_or_default();

        // Milestone 157 depends construction — branched by lockfileVersion:
        // - v9 path: `packages:` entries carry ONLY identity + integrity
        //   metadata plus (for 225/1329 entries in the argo-cd testbed)
        //   a `peerDependencies:` sub-mapping whose VALUES ARE SEMVER
        //   SPECIFIERS (e.g. `^7.0.0`, `^7.4.0 || ^8.0.0-0 <8.0.0`), NOT
        //   resolved versions. Resolved dep-graph edges live EXCLUSIVELY
        //   in `snapshots:`. Reading inline `packages:` sub-mappings on
        //   v9 emits WRONG edges (the specifier's dep-NAME happens to
        //   match a real component so the graph looks plausible, but
        //   the 7-of-8 real edges from snapshots are silently dropped).
        //   Verified empirically 2026-07-03 on argo-cd/ui pre-fix:
        //   `@babel/helper-create-class-features-plugin@7.29.3` was
        //   emitting 1 edge (@babel/core, from the specifier) instead
        //   of 8 (from snapshots).
        // - v6/v7 path: `packages:` entries carry inline `dependencies:`
        //   with RESOLVED versions (matches milestone-147 npm parity).
        //   Walk the 3-section union directly.
        // - Empty on both sides = leaf semantics (FR-005).
        //
        // On v9 the snapshots_lookup HIT case (deps or empty leaf)
        // increments fell_back_count. On v6/v7 fell_back_count stays 0
        // (as expected — no fallback happened).
        let depends: Vec<String> = if is_v9_or_later {
            let canonical = format!("{name}@{version}");
            if let Some(snap_deps) = snapshots_lookup.get(&canonical) {
                fell_back_count += 1;
                snap_deps.clone()
            } else {
                Vec::new()
            }
        } else {
            // Milestone 164 (T005): v6/v7 inline path preserves pre-164
            // bare-name emission (User Story 2 byte-identity guard).
            collect_pnpm_dep_names(
                tbl,
                &mut aliases,
                source_path,
                /* emit_versioned = */ false,
                None,
                None,
            )
        };

        // Milestone 180 T013 + T014 — three-way lifecycle classifier
        // (same shape as npm US1). Precedence:
        //   1. `dev: true`  → Development (m179 FR-015)
        //   2. `optional: true` AND NOT peer-optional → Optional (m180 US2)
        //      + emit `mikebom:optional-derivation = "npm-optional-dependencies"`
        //   3. otherwise → Runtime
        // Peer-optional (`peer && optional`) short-circuits Optional
        // per FR-006 — m178's PROVIDED_DEPENDENCY_OF wins.
        use waybill_common::resolution::LifecycleScope;
        let mut m180_annotations: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        let lifecycle_scope = if is_dev {
            Some(LifecycleScope::Development)
        } else if is_optional && !is_peer {
            m180_annotations.insert(
                "mikebom:optional-derivation".to_string(),
                serde_json::Value::String("npm-optional-dependencies".to_string()),
            );
            Some(LifecycleScope::Optional)
        } else {
            Some(LifecycleScope::Runtime)
        };

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name,
            version,
            arch: None,
            source_path: source_path.to_string(),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
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
            hashes,
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations: m180_annotations,
            binary_role: None,
        });
    }

    // Milestone 159 post-processing (issue #493):
    //
    //   1. Build alias_map for edge rewriting (FR-005) — key = local-
    //      name, value = AliasedIdentity{aliased_name, aliased_version}.
    //   2. Build reverse-map for annotation emission (FR-006) — key =
    //      `<aliased_name>@<aliased_version>` canonical, value = sorted
    //      unique Vec<local_name> of every alias that reached that
    //      canonical (FR-012 multi-alias case).
    //   3. Rewrite each PackageDbEntry.depends via `rewrite_dep_names`.
    //   4. For each PackageDbEntry whose canonical matches the reverse-
    //      map, insert `mikebom:pnpm-alias = <local-name>` into
    //      extra_annotations. Multi-alias emits Value::Array;
    //      single-alias emits Value::String.
    let mut alias_map: super::alias_mapping::AliasMap =
        std::collections::HashMap::new();
    let mut reverse_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for a in &aliases {
        alias_map.entry(a.local_name.clone()).or_insert_with(|| {
            super::alias_mapping::AliasedIdentity {
                aliased_name: a.aliased_name.clone(),
                aliased_version: a.aliased_version.clone(),
            }
        });
        let key = format!("{}@{}", a.aliased_name, a.aliased_version);
        let locals = reverse_map.entry(key).or_default();
        if !locals.contains(&a.local_name) {
            locals.push(a.local_name.clone());
        }
    }
    // Sort each reverse-map value for deterministic annotation order.
    for locals in reverse_map.values_mut() {
        locals.sort();
    }
    // Rewrite edges + attach annotations.
    let alias_count = aliases.len();
    if alias_count > 0 {
        for entry in out.iter_mut() {
            entry.depends =
                super::alias_mapping::rewrite_dep_names(&entry.depends, &alias_map);
            let canonical = format!("{}@{}", entry.name, entry.version);
            if let Some(locals) = reverse_map.get(&canonical) {
                let value = if locals.len() == 1 {
                    serde_json::Value::String(locals[0].clone())
                } else {
                    serde_json::Value::Array(
                        locals
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    )
                };
                entry
                    .extra_annotations
                    .insert("mikebom:pnpm-alias".to_string(), value);
            }
        }
        // Milestone 159 FR-011 info log — emit ONLY when at least one
        // alias was resolved. On no-alias lockfiles, the log line is
        // suppressed to preserve pre-159 byte-identity on unrelated
        // fixtures.
        tracing::info!(
            lockfile_path = %source_path,
            alias_count = alias_count,
            alias_ecosystem = "pnpm",
            "npm-alias resolution completed"
        );
    }

    // Milestone 157 FR-007 info-level diagnostic. Grep-friendly for
    // CI-log analysis. On v6/v7 lockfiles, fell_back_to_snapshots
    // will be 0 (inline path always populated); on well-formed v9
    // lockfiles, it approaches packages_count.
    tracing::info!(
        lockfile = %source_path,
        lockfile_version = %lock_version,
        packages_count = packages.len(),
        snapshots_count = snapshots_lookup.len(),
        fell_back_to_snapshots = fell_back_count,
        // Milestone 164 (T006, FR-009): two new fields extending the
        // existing summary line. Backward-compat for regex consumers.
        multi_version_disambiguated_count = multi_version_disambiguated_count,
        malformed_key_warn_count = malformed_key_warn_count,
        "pnpm-lock parsed"
    );

    // Milestone 157 FR-008 warn-level diagnostic — anomalous v9
    // lockfile shape. Fires when the operator's lockfile claims
    // v9 but has no snapshots section (pnpm's own tools would
    // refuse to install from it, but mikebom's fail-open posture
    // emits the identity-only graph with a diagnostic).
    if is_v9_or_later && snapshots_lookup.is_empty() {
        tracing::warn!(
            lockfile = %source_path,
            lockfile_version = %lock_version,
            "pnpm-lock v9 with no snapshots section — dep-graph will be empty for all non-root components. Check lockfile validity."
        );
    }

    out
}

/// Parse a pnpm package key — `<name>@<version>` or
/// `@<scope>/<name>@<version>` — into (name, version). The last `@`
/// is the version separator; everything before it is the name.
fn parse_pnpm_key(key: &str) -> Option<(String, String)> {
    // Strip any parenthesised peer-dep suffix (e.g. "(react@18.0.0)").
    let key = key.split('(').next().unwrap_or(key);
    // Find the LAST '@' that's after position 0 (position 0 is the
    // scope prefix for @scope/name).
    let search_start = if key.starts_with('@') { 1 } else { 0 };
    let at_idx = key[search_start..].rfind('@').map(|i| i + search_start)?;
    let name = key[..at_idx].to_string();
    let version = key[at_idx + 1..].to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

// -----------------------------------------------------------------------
// Tier B: flat node_modules walk
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn pnpm_lock_v6_style_parses() {
        let yaml = r#"
lockfileVersion: '6.0'
packages:
  /lodash@4.17.21:
    resolution:
      integrity: sha512-MJ7MSJwS1utMxA9QyQLytNDtd+5RGnx+7fIK+4qg9hvLABzzXAIaFMqoD6YFUYaCQPkMInyGdz6TQEsE7bPdCg==
    dev: false
  /eslint@8.0.0:
    dev: true
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "lodash");
        assert_eq!(out[0].version, "4.17.21");
    }

    #[test]
    fn pnpm_lock_scoped_package_parses() {
        let yaml = r#"
lockfileVersion: '6.0'
packages:
  /@angular/core@16.0.0:
    resolution: {}
    dev: false
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "@angular/core");
        assert_eq!(out[0].version, "16.0.0");
        assert_eq!(out[0].purl.as_str(), "pkg:npm/%40angular/core@16.0.0");
    }

    #[test]
    fn pnpm_key_parser_handles_peer_suffix() {
        // v9 adds peer-dep suffixes: `react-dom@18.0.0(react@18.0.0)`.
        let (name, version) = parse_pnpm_key("react-dom@18.0.0(react@18.0.0)").unwrap();
        assert_eq!(name, "react-dom");
        assert_eq!(version, "18.0.0");
    }

    // ============================================================
    // Milestone 157 unit tests (SC-007 floor ≥8; 9 tests total
    // after F1 remediation added test #9 for SC-005 behavioral
    // verification). All 9 fn names begin with pnpm_v6_ / pnpm_v9_
    // / pnpm_walks_ for SC-007 grep compatibility.
    // ============================================================

    /// Helper: find first emitted entry by name.
    fn entry_by_name<'a>(entries: &'a [PackageDbEntry], name: &str) -> Option<&'a PackageDbEntry> {
        entries.iter().find(|e| e.name == name)
    }

    #[test]
    fn pnpm_v9_minimal_dependencies_only_emits_edge() {
        // Minimal v9 fixture: packages: identity + snapshots: single
        // dependencies edge. Assert the edge appears in depends[].
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbb}

snapshots:
  foo@1.0.0:
    dependencies:
      bar: 2.0.0
  bar@2.0.0: {}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        // Milestone 164 (T003): v9 depends now carry version-qualified
        // disambiguation form so the downstream `name_to_purl` lookup
        // picks the correct version when multi-version cases arise.
        assert_eq!(foo.depends, vec!["bar 2.0.0"]);
    }

    #[test]
    fn pnpm_v9_empty_snapshot_body_leaf_node() {
        // SC-004 + FR-005: snapshots entry with empty body → empty depends.
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}

snapshots:
  foo@1.0.0: {}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        assert!(foo.depends.is_empty(), "leaf node MUST have empty depends; got {:?}", foo.depends);
    }

    #[test]
    fn pnpm_v9_peer_dep_suffix_normalized_in_key_and_value() {
        // SC-003 + FR-003: peer-dep suffixes on snapshot KEY and dep VALUE
        // both normalize via parse_pnpm_key to canonical name@version.
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  baz@3.0.0:
    resolution: {integrity: sha512-cccc}

snapshots:
  foo@1.0.0(bar@2.0.0):
    dependencies:
      baz: 3.0.0(qux@4.0.0)
  baz@3.0.0: {}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        // Identity peer-suffix stripped from PURL.
        assert_eq!(foo.purl.as_str(), "pkg:npm/foo@1.0.0");
        // Value peer-suffix stripped from edge target. Milestone 164
        // (T003): v9 depends carry version-qualified disambiguation form.
        assert_eq!(foo.depends, vec!["baz 3.0.0"]);
    }

    #[test]
    fn pnpm_v9_orphaned_snapshot_skipped() {
        // FR-006: snapshot with no matching packages entry → skip.
        let yaml = r#"
lockfileVersion: '9.0'

packages: {}

snapshots:
  foo@1.0.0:
    dependencies:
      bar: 2.0.0
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        assert!(out.is_empty(), "orphan snapshot MUST NOT emit; got {:?}", out);
    }

    #[test]
    fn pnpm_v9_all_three_sub_mappings_union_with_dedup() {
        // FR-002 + Q1 clarification: union of dependencies +
        // peerDependencies + optionalDependencies with defensive dedup.
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  a@1.0.0:
    resolution: {}
  b@2.0.0:
    resolution: {}
  c@3.0.0:
    resolution: {}
  shared@5.0.0:
    resolution: {}

snapshots:
  foo@1.0.0:
    dependencies:
      a: 1.0.0
      shared: 5.0.0
    peerDependencies:
      b: 2.0.0
      shared: 5.0.0
    optionalDependencies:
      c: 3.0.0
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        // Sorted + deduped union (shared@5.0.0 appears once). Milestone
        // 164 (T003): v9 depends carry version-qualified form.
        assert_eq!(
            foo.depends,
            vec!["a 1.0.0", "b 2.0.0", "c 3.0.0", "shared 5.0.0"]
        );
    }

    #[test]
    fn pnpm_v6_v7_inline_peer_and_optional_now_emit() {
        // FR-004 + Q1: v6/v7 inline path now walks 3 sub-mappings.
        // No snapshots section — pure v6-style fixture.
        let yaml = r#"
lockfileVersion: '6.0'

packages:
  /foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
    dependencies:
      a: 1.0.0
    peerDependencies:
      b: 2.0.0
    optionalDependencies:
      c: 3.0.0
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        assert_eq!(foo.depends, vec!["a", "b", "c"]);
    }

    #[test]
    fn pnpm_v9_snapshots_authoritative_ignoring_packages_specifiers() {
        // Empirical bug 2026-07-03 (post-T014 argo-cd/ui audit): on a
        // real pnpm v9 lockfile, `packages:` entries carry ONLY
        // identity/integrity metadata plus (for a subset) a
        // `peerDependencies:` sub-mapping whose values are SEMVER
        // SPECIFIERS, not resolved versions. Resolved dep-graph edges
        // live EXCLUSIVELY in `snapshots:`. Reading `packages:` inline
        // sub-mappings on v9 emits WRONG edges (the specifier's
        // dep-NAME may coincidentally match a real component). Correct
        // behavior: on v9, ALWAYS use snapshots and IGNORE inline
        // packages: sub-mappings.
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
    peerDependencies:
      only-specifier: ^7.0.0

snapshots:
  foo@1.0.0:
    dependencies:
      only-snapshots: 2.0.0
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        assert_eq!(
            foo.depends,
            vec!["only-snapshots 2.0.0"],
            "v9 MUST read edges from snapshots and MUST NOT emit \
             the specifier-only edge from packages:.peerDependencies. \
             Milestone 164 (T003): depends carry version-qualified form."
        );
    }

    #[test]
    fn pnpm_walks_same_dep_sections_as_package_lock_non_dev() {
        // SC-011 parity: PNPM_DEP_SECTIONS matches the non-dev subset
        // of package_lock.rs's 4-section walk. The `dev: true` boolean
        // handles pnpm's dev-status axis; no devDependencies: sub-
        // mapping exists on individual pnpm entries by lockfile design.
        assert_eq!(
            PNPM_DEP_SECTIONS,
            &["dependencies", "peerDependencies", "optionalDependencies"],
            "SC-011: PNPM_DEP_SECTIONS drift from expected parity set"
        );
    }

    #[test]
    fn pnpm_v9_no_snapshots_scans_cleanly_with_empty_deps() {
        // F1 remediation, SC-005 behavioral verification: v9 lockfile
        // with no `snapshots:` key at all. Parser MUST return cleanly
        // (no panic) and emit components with empty depends. The
        // FR-008 tracing::warn! side effect is documented in FR-008
        // for operator grep; automated log-string capture is out of
        // scope per SC-005's downgraded automation claim.
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbb}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false);
        assert_eq!(out.len(), 2);
        for entry in &out {
            assert!(
                entry.depends.is_empty(),
                "v9 with no snapshots MUST emit empty depends; {} got {:?}",
                entry.name,
                entry.depends
            );
        }
    }

    // ─── Milestone 159 alias resolution tests (T014, T015a, T016) ───

    /// T014 — real test-podman-desktop-shape aliases: `react-helmet-async`,
    /// `react-loadable`, `string-width-cjs`, `strip-ansi-cjs` all resolve
    /// to their aliased canonical PURLs; the local-name PURLs are NOT
    /// emitted; each aliased entry carries `mikebom:pnpm-alias =
    /// <local-name>`; the depender's depends list contains the aliased
    /// canonical names.
    #[test]
    fn m159_pnpm_alias_test_podman_desktop_shape() {
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  docusaurus-core@3.10.1:
    resolution: {integrity: sha512-aaaa}
  '@slorber/react-helmet-async@1.3.0':
    resolution: {integrity: sha512-bbbb}
  '@docusaurus/react-loadable@6.0.0':
    resolution: {integrity: sha512-cccc}
  cliui@8.0.2:
    resolution: {integrity: sha512-dddd}
  string-width@4.2.3:
    resolution: {integrity: sha512-eeee}
  strip-ansi@6.0.1:
    resolution: {integrity: sha512-ffff}

snapshots:
  docusaurus-core@3.10.1:
    dependencies:
      react-helmet-async: '@slorber/react-helmet-async@1.3.0'
      react-loadable: '@docusaurus/react-loadable@6.0.0'
  cliui@8.0.2:
    dependencies:
      string-width-cjs: string-width@4.2.3
      strip-ansi-cjs: strip-ansi@6.0.1
  '@slorber/react-helmet-async@1.3.0':
    resolution: {integrity: sha512-bbbb}
  '@docusaurus/react-loadable@6.0.0':
    resolution: {integrity: sha512-cccc}
  string-width@4.2.3: {}
  strip-ansi@6.0.1: {}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/test.yaml", false);

        // Aliased-canonical components MUST be emitted.
        let by_name: std::collections::HashMap<String, &PackageDbEntry> =
            out.iter().map(|e| (e.name.clone(), e)).collect();
        assert!(
            by_name.contains_key("@slorber/react-helmet-async"),
            "aliased component @slorber/react-helmet-async missing"
        );
        assert!(
            by_name.contains_key("@docusaurus/react-loadable"),
            "aliased component @docusaurus/react-loadable missing"
        );
        assert!(by_name.contains_key("string-width"), "aliased component string-width missing");
        assert!(by_name.contains_key("strip-ansi"), "aliased component strip-ansi missing");

        // Local-name components MUST NOT be emitted.
        assert!(
            !by_name.contains_key("react-helmet-async"),
            "local-name react-helmet-async MUST NOT be emitted"
        );
        assert!(
            !by_name.contains_key("string-width-cjs"),
            "local-name string-width-cjs MUST NOT be emitted"
        );

        // Aliased entries carry `mikebom:pnpm-alias = <local-name>`.
        let slorber = by_name["@slorber/react-helmet-async"];
        assert_eq!(
            slorber.extra_annotations.get("mikebom:pnpm-alias"),
            Some(&serde_json::Value::String("react-helmet-async".to_string())),
        );
        let sw = by_name["string-width"];
        assert_eq!(
            sw.extra_annotations.get("mikebom:pnpm-alias"),
            Some(&serde_json::Value::String("string-width-cjs".to_string())),
        );

        // The depender's depends list references the aliased identity.
        // Milestone 164 (T003 + T007): v9 depends now carry the
        // version-qualified disambiguation form; milestone-159 alias
        // substitution preserves the version segment through the rename
        // (per T007 `rewrite_dep_names` update).
        let docusaurus = by_name["docusaurus-core"];
        assert!(docusaurus.depends.contains(&"@slorber/react-helmet-async 1.3.0".to_string()));
        assert!(docusaurus.depends.contains(&"@docusaurus/react-loadable 6.0.0".to_string()));
        assert!(!docusaurus.depends.contains(&"react-helmet-async".to_string()));

        let cliui = by_name["cliui"];
        assert!(cliui.depends.contains(&"string-width 4.2.3".to_string()));
        assert!(cliui.depends.contains(&"strip-ansi 6.0.1".to_string()));
        assert!(!cliui.depends.contains(&"string-width-cjs".to_string()));
    }

    /// T015a (analyze finding A2) — FR-012 multi-alias: same canonical
    /// component reached via TWO different local-names emits a
    /// Value::Array of local-names in `mikebom:pnpm-alias`.
    #[test]
    fn m159_pnpm_multi_alias_emits_array() {
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  peer-a@1.0.0:
    resolution: {integrity: sha512-aaaa}
  peer-b@1.0.0:
    resolution: {integrity: sha512-bbbb}
  '@slorber/react-helmet-async@1.3.0':
    resolution: {integrity: sha512-cccc}

snapshots:
  peer-a@1.0.0:
    dependencies:
      helmet-shim: '@slorber/react-helmet-async@1.3.0'
  peer-b@1.0.0:
    dependencies:
      react-helmet-async: '@slorber/react-helmet-async@1.3.0'
  '@slorber/react-helmet-async@1.3.0':
    resolution: {integrity: sha512-cccc}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/test.yaml", false);
        let by_name: std::collections::HashMap<String, &PackageDbEntry> =
            out.iter().map(|e| (e.name.clone(), e)).collect();
        let helmet = by_name["@slorber/react-helmet-async"];
        let anno = helmet.extra_annotations.get("mikebom:pnpm-alias").unwrap();
        // Expected: Value::Array of both locals, sorted.
        let arr = anno.as_array().expect("annotation must be Value::Array for multi-alias");
        let values: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(values, vec!["helmet-shim", "react-helmet-async"]);
    }

    /// T016 — no-alias pnpm-lock is byte-identical to pre-159 behavior:
    /// no `mikebom:pnpm-alias` annotations added on any entry.
    #[test]
    fn m159_pnpm_no_alias_no_annotation() {
        let yaml = r#"
lockfileVersion: '9.0'

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbb}

snapshots:
  foo@1.0.0:
    dependencies:
      bar: 2.0.0
  bar@2.0.0: {}
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&parsed, "/no-alias.yaml", false);
        for entry in &out {
            assert!(
                !entry.extra_annotations.contains_key("mikebom:pnpm-alias"),
                "no-alias lockfile MUST NOT add mikebom:pnpm-alias; {} did",
                entry.name
            );
        }
    }

    // -----------------------------------------------------------------
    // Milestone 164 (T008-T014, implements m164 FR-001 through FR-010)
    // Cross-workspace multi-version disambiguation unit tests.
    // -----------------------------------------------------------------

    fn make_yaml_mapping(yaml: &str) -> serde_yaml::Mapping {
        let v: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        v.as_mapping().unwrap().clone()
    }

    /// T008: `emit_versioned=true` produces `"<name> <version>"` form
    /// per SC-007 (a) + (b).
    #[test]
    fn t008_collect_pnpm_dep_names_emit_versioned_true_produces_versioned() {
        let tbl = make_yaml_mapping(
            r#"
dependencies:
  foo: 1.2.3(peer@4.5.6)
"#,
        );
        let mut aliases = Vec::new();
        let deps = collect_pnpm_dep_names(&tbl, &mut aliases, "/test", true, None, None);
        assert_eq!(deps, vec!["foo 1.2.3".to_string()]);
    }

    /// T009: `emit_versioned=false` preserves pre-164 bare-name form
    /// per SC-007 (b) + US2 regression guard.
    #[test]
    fn t009_collect_pnpm_dep_names_emit_versioned_false_preserves_bare_name() {
        let tbl = make_yaml_mapping(
            r#"
dependencies:
  foo: 1.2.3(peer@4.5.6)
"#,
        );
        let mut aliases = Vec::new();
        let deps = collect_pnpm_dep_names(&tbl, &mut aliases, "/test", false, None, None);
        assert_eq!(deps, vec!["foo".to_string()]);
    }

    /// T010: FR-008 malformed-key fallback. Per research R3 option (a),
    /// verified empirically 2026-07-05 (`malformed_key_warn_count=0`
    /// on live podman-desktop): `parse_pnpm_key` returns `None` (never
    /// `Some((name, ""))`) for empty-version keys, so the WARN branch
    /// is defensive-only. This test asserts THAT parser property —
    /// the FR-008 branch fires only if `parse_pnpm_key` ever regresses
    /// to returning `Some(_, "")`. Any future change to `parse_pnpm_key`
    /// that violates this invariant will surface here.
    #[test]
    fn t010_parse_pnpm_key_never_returns_empty_version() {
        // Cases that should return None (either malformed or unparseable):
        let none_cases = &[
            "foo@",         // empty version
            "@scope/foo@",  // scoped empty version
            "@scope/foo",   // no @version separator (scope-only)
            "foo",          // no @version separator
            "",             // empty
        ];
        for case in none_cases {
            let result = parse_pnpm_key(case);
            assert!(
                result.is_none() || result.as_ref().map(|(_, v)| !v.is_empty()).unwrap_or(true),
                "parse_pnpm_key({case:?}) returned Some with empty version — \
                 milestone-164 FR-008 WARN branch would fire on this input. \
                 Fix `parse_pnpm_key` to return None on empty-version keys."
            );
        }
    }

    /// T011: v9 `build_snapshots_lookup` emits versioned form + threads
    /// counters per SC-007 (b).
    #[test]
    fn t011_build_snapshots_lookup_emits_versioned_for_v9() {
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbb}
snapshots:
  foo@1.0.0:
    dependencies:
      bar: 2.0.0
  bar@2.0.0: {}
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let mut aliases = Vec::new();
        let mut versioned = 0usize;
        let mut warned = 0usize;
        let lookup =
            build_snapshots_lookup(&root, &mut aliases, "/test.yaml", &mut versioned, &mut warned);
        assert_eq!(lookup.get("foo@1.0.0"), Some(&vec!["bar 2.0.0".to_string()]));
        assert!(versioned >= 1, "versioned_counter must increment on v9");
        assert_eq!(warned, 0, "no malformed keys expected on well-formed fixture");
    }

    /// T012: multi-version edges resolve correctly — parents pointing
    /// at different versions of the same package. Per SC-007 (f).
    #[test]
    fn t012_parse_pnpm_lock_multi_version_edges_resolve_correctly() {
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  foo@2.0.0:
    resolution: {integrity: sha512-bbbb}
  parent-a@1.0.0:
    resolution: {integrity: sha512-cccc}
  parent-b@1.0.0:
    resolution: {integrity: sha512-dddd}
snapshots:
  foo@1.0.0: {}
  foo@2.0.0: {}
  parent-a@1.0.0:
    dependencies:
      foo: 1.0.0
  parent-b@1.0.0:
    dependencies:
      foo: 2.0.0
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", false);
        // (a) both foo@1.0.0 AND foo@2.0.0 emitted
        let foo_versions: Vec<&str> = out
            .iter()
            .filter(|e| e.name == "foo")
            .map(|e| e.version.as_str())
            .collect();
        assert!(foo_versions.contains(&"1.0.0"), "foo@1.0.0 missing: {foo_versions:?}");
        assert!(foo_versions.contains(&"2.0.0"), "foo@2.0.0 missing: {foo_versions:?}");
        // (b) parent-a → foo 1.0.0 (versioned form for downstream disambiguation)
        let parent_a = entry_by_name(&out, "parent-a").expect("parent-a emitted");
        assert_eq!(parent_a.depends, vec!["foo 1.0.0"]);
        // (c) parent-b → foo 2.0.0
        let parent_b = entry_by_name(&out, "parent-b").expect("parent-b emitted");
        assert_eq!(parent_b.depends, vec!["foo 2.0.0"]);
    }

    /// T013: FR-005 — emitted PURL NEVER contains peer-dep suffix `(`.
    #[test]
    fn t013_parse_pnpm_lock_purl_never_includes_peer_dep_suffix() {
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbb}
snapshots:
  foo@1.0.0(bar@2.0.0):
    dependencies:
      bar: 2.0.0
  bar@2.0.0: {}
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", false);
        for entry in &out {
            assert!(
                !entry.purl.as_str().contains('('),
                "FR-005 violated: emitted PURL contains `(`: {}",
                entry.purl.as_str()
            );
        }
    }

    /// T014: FR-010 — `peerDependencies` inclusion behavior unchanged
    /// by milestone 164 (still part of the SC-004 3-section union per
    /// milestone 157 Q1). Milestone 164 does NOT touch peer-dep
    /// semantics; verifies pre-164 handling persists.
    #[test]
    fn t014_peer_dependencies_handling_unchanged_after_164() {
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
  peer-dep@2.0.0:
    resolution: {integrity: sha512-bbbb}
snapshots:
  foo@1.0.0:
    peerDependencies:
      peer-dep: 2.0.0
  peer-dep@2.0.0: {}
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        // Per milestone 157 Q1, peerDependencies is part of the union;
        // milestone 164 preserves that behavior AND applies the new
        // disambiguation-key form to peer-dep edges (uniform treatment
        // across all 3 sub-sections).
        assert_eq!(foo.depends, vec!["peer-dep 2.0.0"]);
    }

    /// T016 (US2): pnpm v6/v7 inline path emits bare-name form (NO
    /// version-qualified disambiguation). Confirms FR-002 byte-identity
    /// guard for pre-v9 lockfiles — even though `collect_pnpm_dep_names`
    /// now has an `emit_versioned` parameter, the v6/v7 code path calls
    /// it with `emit_versioned=false` so pre-164 output shape is
    /// preserved. Milestone 164 (T005 + T016 US2).
    #[test]
    fn t016_v6_v7_inline_path_emits_bare_names() {
        let yaml = r#"
lockfileVersion: '6.0'
packages:
  /foo@1.0.0:
    resolution: {integrity: sha512-aaaa}
    dependencies:
      bar: 2.0.0
      baz: 3.0.0
    peerDependencies:
      qux: 4.0.0
    optionalDependencies:
      opt: 5.0.0
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", false);
        let foo = entry_by_name(&out, "foo").expect("foo emitted");
        // Pre-164 bare-name form preserved for v6/v7 lockfiles.
        assert_eq!(
            foo.depends,
            vec!["bar", "baz", "opt", "qux"],
            "v6/v7 inline path MUST emit bare-name form (US2 byte-identity guard); \
             got {:?}",
            foo.depends
        );
        // Guard: no versioned form leaked through.
        for dep in &foo.depends {
            assert!(
                !dep.contains(' '),
                "v6/v7 dep `{dep}` MUST NOT contain space — versioned form leaked from m164 path"
            );
        }
    }

    // ------------------------------------------------------------------
    // Milestone 180 US2 — optional-dep classification for pnpm reader.
    // ------------------------------------------------------------------

    #[test]
    fn pnpm_optional_true_populates_lifecycle_scope_optional() {
        // Milestone 180 T015 — pnpm v9 packages entry with
        // `optional: true` gets LifecycleScope::Optional + the m180
        // derivation annotation.
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  /fsevents@2.3.3:
    resolution: {integrity: sha512-testtest}
    optional: true
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", true);
        let fsevents = entry_by_name(&out, "fsevents").expect("fsevents emitted");
        assert_eq!(
            fsevents.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            fsevents
                .extra_annotations
                .get("mikebom:optional-derivation"),
            Some(&serde_json::Value::String(
                "npm-optional-dependencies".to_string()
            ))
        );
    }

    #[test]
    fn pnpm_dev_true_wins_over_optional() {
        // Milestone 180 T015 / FR-015 — dev wins over optional even
        // when both flags are set on the same pnpm entry.
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  /some-dev-opt@1.0.0:
    resolution: {integrity: sha512-devopt}
    dev: true
    optional: true
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", true);
        let entry = entry_by_name(&out, "some-dev-opt").expect("entry emitted");
        assert_eq!(
            entry.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Development)
        );
        assert!(
            !entry
                .extra_annotations
                .contains_key("mikebom:optional-derivation")
        );
    }

    #[test]
    fn pnpm_peer_optional_stays_peer_not_optional() {
        // Milestone 180 T015 / FR-006 — pnpm entry with BOTH `peer:
        // true` AND `optional: true` short-circuits to Runtime; m178's
        // PROVIDED_DEPENDENCY_OF fires separately.
        let yaml = r#"
lockfileVersion: '9.0'
packages:
  /react@18.3.1:
    resolution: {integrity: sha512-reactint}
    peer: true
    optional: true
"#;
        let root: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let out = parse_pnpm_lock(&root, "/test.yaml", true);
        let react = entry_by_name(&out, "react").expect("react emitted");
        assert_ne!(
            react.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional),
            "peer-optional entry MUST NOT be classified as Optional (FR-006)"
        );
        assert!(
            !react
                .extra_annotations
                .contains_key("mikebom:optional-derivation"),
            "peer-optional entry MUST NOT carry mikebom:optional-derivation (FR-006)"
        );
    }
}
