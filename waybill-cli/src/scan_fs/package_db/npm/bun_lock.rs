//! `bun.lock` parser — milestone 106 US2 (issue #278).
//!
//! Parses [Bun](https://bun.sh/)'s text-format lockfile (JSONC; binary
//! `bun.lockb` is explicitly out of scope per the issue). Sibling to
//! `package_lock.rs` (npm v2/v3) and `pnpm_lock.rs` (pnpm). Invoked
//! from [`super::read`] per-project-root after the existing
//! lockfile readers; tier-A authority same as npm + pnpm.
//!
//! Schema (Bun 1.2+):
//!
//! ```jsonc
//! // bun: lockfileVersion: 1
//! {
//!   "lockfileVersion": 1,
//!   "workspaces": {
//!     "": { "name": "root-name", "dependencies": {...} },
//!     "packages/web": { "name": "@my/web", "dependencies": {"@my/shared": "workspace:*"} },
//!     "packages/shared": { "name": "@my/shared" }
//!   },
//!   "packages": {
//!     "lodash": ["lodash@4.17.21", "sha512-..."],
//!     "@my/web": ["@my/web@workspace:packages/web"],
//!     "@my/shared": ["@my/shared@workspace:packages/shared"]
//!   },
//!   "overrides": { "lodash": "4.17.21" }
//! }
//! ```
//!
//! Per the Clarification Q1 of milestone 106, workspace handling
//! emits one main-module per workspace member + a synthetic
//! workspace-root + intra-workspace dependency edges.

use std::collections::HashSet;
use std::path::Path;

use super::super::workspace::{synthesize_workspace_root, workspace_root_name};
use super::super::PackageDbEntry;
use super::build_npm_purl;

/// Placeholder version used when a workspace member's `package.json`
/// is missing/unreadable or has no `version` field. Keeps the
/// resulting PURL well-formed without pretending to know the real
/// version. Workspace members are unversioned-by-design in many
/// monorepo setups, so this is a deliberate sentinel.
const WORKSPACE_MEMBER_VERSION_PLACEHOLDER: &str = "0.0.0";

/// Milestone 667 helper: split a `bun.lock` `packages`-map key into
/// scope-atomic segments. A segment is either a bare `<name>` (one
/// slash-delimited component) OR a `@<scope>/<name>` (two slash-
/// delimited components treated as ONE atomic unit).
///
/// Examples:
/// - `"foo/bar/baz"` → `["foo", "bar", "baz"]`
/// - `"lodash"` → `["lodash"]`
/// - `"@scope/name"` → `["@scope/name"]`
/// - `"@fast-csv/format/@types/node"` → `["@fast-csv/format", "@types/node"]`
///
/// This DIFFERS from bare `str::split('/')`: scoped-name segments
/// occupy TWO slash-delimited components each, and splitting naively
/// on `/` mis-locates scope boundaries. Every consumer of bun.lock
/// key paths (both the resolver and the key-path→disambiguator
/// index building) MUST use this segmentation, never `split('/')`.
///
/// Pure function; no I/O; deterministic.
fn segment_bun_key(key: &str) -> Vec<&str> {
    let bytes = key.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        // If the segment begins with '@', it's a scope-atomic segment:
        // skip past the FIRST '/' (scope/name divider) before searching
        // for the NEXT '/' (segment boundary).
        if bytes[i] == b'@' {
            while i < bytes.len() && bytes[i] != b'/' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // step past the scope/name divider
            }
        }
        // Advance to the next '/' or end.
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        segments.push(&key[start..i]);
        if i < bytes.len() {
            i += 1; // step past the segment divider
            start = i;
        }
    }
    segments
}

/// Milestone 667 resolver: given a parent's `bun.lock` `packages`-map
/// key and a dep-name declared in that parent's metadata object,
/// walk the parent's key path from most-specific to root and return
/// the first `<prefix>/<dep_name>` (or bare `<dep_name>` at root)
/// that exists in `packages_keys`. Returns `None` if no candidate hits.
///
/// This is the bun-native equivalent of npm's node_modules-tree
/// resolution walk (`package_lock.rs::resolve_dep_via_node_modules_walk`),
/// adapted for bun's `--linker=isolated` install-chain key encoding.
/// Bun keys non-hoisted packages by their install chain (e.g., the
/// same package name at two versions ends up at
/// `"foo/bar/minimatch"` and `"baz/minimatch"`); the resolver's most-
/// specific-prefix-first walk picks the correct version copy per
/// bun's install-chain semantics.
///
/// Test vectors (T014-T017 assert these):
/// - `parent_key="lodash"`, `dep_name="chalk"`, `packages_keys={"chalk"}`
///   → candidates walked: `"lodash/chalk"`, `"chalk"` → returns `Some("chalk")`.
/// - `parent_key="foo/bar/baz"`, `dep_name="@scope/pkg"`, `packages_keys={"foo/bar/@scope/pkg"}`
///   → candidates walked: `"foo/bar/baz/@scope/pkg"`, `"foo/bar/@scope/pkg"`, `"foo/@scope/pkg"`, `"@scope/pkg"`
///   → returns `Some("foo/bar/@scope/pkg")`.
/// - `parent_key="@fast-csv/format/@types/node"`, `dep_name="tslib"`, `packages_keys={"@fast-csv/format/tslib"}`
///   → candidates walked: `"@fast-csv/format/@types/node/tslib"`, `"@fast-csv/format/tslib"`, `"tslib"`
///   → returns `Some("@fast-csv/format/tslib")`.
///
/// See `specs/667-bun-lock-edges/research.md` §R2 for the algorithm
/// rationale + `specs/667-bun-lock-edges/contracts/depends-emission.md` C2
/// for the correctness contract.
///
/// Note on prior R2 authoring: research.md's first test vector originally
/// listed an intermediate candidate `"@fast-csv/format/@types/tslib"` for the
/// first case above — that was a scope-boundary miscount in my authoring;
/// the correct walk drops that step because scoped-name segments are
/// atomic. Implementation follows the correct walk (3 candidates for a
/// 2-segment parent key).
///
/// Pure function; no I/O; deterministic.
fn resolve_bun_key(
    parent_key: &str,
    dep_name: &str,
    packages_keys: &std::collections::HashSet<&str>,
) -> Option<String> {
    let segments = segment_bun_key(parent_key);
    // Walk from most-specific (level = segments.len()) down to root
    // (level = 0), inclusive. First match wins.
    for level in (0..=segments.len()).rev() {
        let candidate = if level == 0 {
            dep_name.to_string()
        } else {
            format!("{}/{}", segments[..level].join("/"), dep_name)
        };
        if packages_keys.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// Read `<rootfs>/bun.lock` if present. Returns None when absent or
/// unparseable. Pre-processes the file through the JSONC stripper to
/// remove `//` line comments and `/* */` block comments before
/// handing the result to `serde_json::from_str` — every real-world
/// `bun.lock` has at least the top-of-file `// bun: lockfileVersion: 1`
/// marker.
pub(super) fn read_bun_lock(
    rootfs: &Path,
    _include_dev: bool,
) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("bun.lock");
    let text = std::fs::read_to_string(&path).ok()?;
    let stripped = super::jsonc::strip_comments(&text);
    let parsed: serde_json::Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "bun.lock JSONC parse failed; skipping (FR-010 warn-and-continue)"
            );
            return None;
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    Some(parse_bun_lock(&parsed, &source_path, rootfs))
}

/// Parse an already-deserialized `bun.lock` JSON value. Public-in-
/// module for unit testing. `rootfs` is used to read workspace member
/// `package.json` files for the version field; tests can pass a
/// tempdir.
pub(crate) fn parse_bun_lock(
    root: &serde_json::Value,
    source_path: &str,
    rootfs: &Path,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();

    // Extract `overrides` map; entries here win over any version
    // declared in `packages`. We apply at registry-package emission
    // time (the un-overridden version is NOT also emitted).
    let overrides: std::collections::BTreeMap<String, String> = root
        .get("overrides")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Step 1: detect workspace from the `workspaces` map. Skip the
    // root entry (key="") — capture its `name` field for the
    // synthetic workspace-root component, and record each member's
    // name in a set so we can both (a) tag them with
    // component-role: "main-module" and (b) skip them in the
    // packages-map walk below.
    let mut workspace_root_name_field: Option<String> = None;
    let mut workspace_member_names: HashSet<String> = HashSet::new();

    if let Some(workspaces) = root.get("workspaces").and_then(|v| v.as_object()) {
        for (path, info) in workspaces {
            let name = info.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            if path.is_empty() {
                workspace_root_name_field = name;
                continue;
            }
            let Some(member_name) = name else { continue };
            workspace_member_names.insert(member_name.clone());

            // Read the member's package.json for the version field.
            // Absent / unreadable → use the placeholder.
            let member_pkg_json_path = rootfs.join(path).join("package.json");
            let version = std::fs::read_to_string(&member_pkg_json_path)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v.get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| WORKSPACE_MEMBER_VERSION_PLACEHOLDER.to_string());

            // Intra-workspace edges: walk the member's `dependencies`
            // field; any value starting with `workspace:` is a
            // sibling-workspace dep. Record its NAME (the key) in
            // `depends`.
            let depends: Vec<String> = info
                .get("dependencies")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(_, v)| {
                            v.as_str()
                                .map(|s| s.starts_with("workspace:"))
                                .unwrap_or(false)
                        })
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();

            let Some(purl) = build_npm_purl(&member_name, &version) else {
                tracing::warn!(
                    workspace_member = %member_name,
                    "bun workspace member produced invalid PURL; skipping"
                );
                continue;
            };

            let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            extra.insert(
                "waybill:component-role".to_string(),
                serde_json::Value::String("main-module".to_string()),
            );

            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: member_name.clone(),
                version,
                arch: None,
                source_path: source_path.to_string(),
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
                extra_annotations: extra,
                binary_role: None,
            });
        }
    }

    // ════════════════════════════════════════════════════════════════
    // MILESTONE 667 — bun.lock transitive-edge emission (closes #723)
    // ════════════════════════════════════════════════════════════════
    //
    // Pre-fix state (m106 / #723): the reader emitted every component
    // the lockfile declared but populated `PackageDbEntry.depends`
    // with an empty vec on every non-workspace entry. Every parent
    // → child edge declared inside a `packages`-map entry's own
    // metadata object was silently dropped, so downstream orphan
    // classification labeled every transitively-reached package
    // `waybill:orphan-reason = "hoisted-unused"`. The reporter's
    // spot-check on a real Bun monorepo showed 1092/1312 components
    // orphaned; every `<foo> → multer → busboy → streamsearch`-style
    // chain broke at the first hop.
    //
    // Post-fix (m667): three passes appended after Step 2's component
    // emission, all strictly additive to the pre-fix code (FR-007 no
    // workspace regression + FR-008 zero new components + FR-009 no
    // touch on inventory pass — the sole modification to pre-m667 is
    // the single `let step2_start = out.len();` line below).
    //
    // Pass 1 (T009): build lookup tables
    //   • `packages_key_index: HashMap<&str, String>` — every
    //     packages-map key mapped to its emitted component's
    //     `"<name> <version>"` disambiguation string (matching
    //     `package_lock.rs:261` convention; see `scan_fs/mod.rs:635-644`
    //     for the graph builder's secondary `name_to_purl` key).
    //   • `parent_key_to_out_idx: HashMap<&str, usize>` — every key
    //     mapped to its position in `out` for O(1) Pass 2 mutation.
    //
    // Pass 2 (T010 + T011): walk each parent's metadata object at
    // tuple position [2], resolve each dep_name via the R2 scope-
    // aware key-path walker (`resolve_bun_key` at line 95), append
    // resolved `"<name> <version>"` strings to the parent's `depends`
    // via a per-parent BTreeMap-backed dedup set (matches m147 issue
    // #262 precedent). Warn-and-drop on 4 edge-drop reasons per R5
    // (metadata_absent / metadata_malformed / unresolved / empty_range).
    // Track per-target optionality in `target_opt_state`.
    //
    // Finalize (T012): apply optional-scope tagging to targets reached
    // EXCLUSIVELY via optional/optional-peers sections. Sets
    // `lifecycle_scope = Some(LifecycleScope::Optional)` + inserts
    // `waybill:optional-derivation = "bun-optional-dependencies"`
    // (or `"bun-optional-peers"` when exclusively peers) — mirrors
    // m180 `package_lock.rs:318-329` verbatim. Data-model V5 (hard
    // beats optional) + V6 (dep-derivation string wins over peer).
    //
    // Contract cross-references (see `specs/667-bun-lock-edges/
    // contracts/depends-emission.md`):
    //   C1 — edge-source completeness (all 4 sections)
    //   C2 — resolver correctness (R2 test vectors)
    //   C3 — `<name> <version>` disambiguation format
    //   C4 — multi-version integrity
    //   C5 — FR-011 warn-and-drop
    //   C6 — FR-008 component-set invariant
    //   C7 — optional-scope precedence
    //   C8 — workspace-path preservation
    //   C9 — override interaction
    // ════════════════════════════════════════════════════════════════

    // Milestone 667 T009: capture the `out.len()` right before Step 2
    // begins so the m667 Pass 1 below can compute per-entry `out`
    // indices. This is a single `let` binding outside the emission
    // loop; the loop itself remains byte-identical to pre-m667
    // (FR-009).
    let step2_start = out.len();

    // Step 2: walk the `packages` map. Each value is an array whose
    // FIRST element is the canonical `<name>@<source-spec>` string.
    // Source-specs:
    //   - Semver (e.g. `4.17.21`)            → registry package
    //   - `workspace:<path>`                 → workspace member (skip — already emitted in step 1)
    //   - `https://...` / `git+...`          → URL / git source (treat as registry for now)
    if let Some(packages) = root.get("packages").and_then(|v| v.as_object()) {
        for (_key, value) in packages {
            let Some(spec) = value
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            // Split on the rightmost `@` so scoped names like
            // `@types/node@22.5.0` parse correctly.
            let Some((name, source_spec)) = spec.rsplit_once('@') else {
                continue;
            };
            if name.is_empty() || source_spec.is_empty() {
                continue;
            }
            // Workspace entries already emitted in step 1.
            if source_spec.starts_with("workspace:") {
                continue;
            }
            // Skip workspace members that also appear in packages
            // (some bun versions duplicate them).
            if workspace_member_names.contains(name) {
                continue;
            }

            // Override resolution: if an `overrides` entry names this
            // package, the override version wins. The un-overridden
            // version is NOT also emitted.
            let resolved_version = overrides
                .get(name)
                .cloned()
                .unwrap_or_else(|| source_spec.to_string());

            let Some(purl) = build_npm_purl(name, &resolved_version) else {
                tracing::warn!(
                    package = %name,
                    version = %resolved_version,
                    "bun.lock packages entry produced invalid PURL; skipping"
                );
                continue;
            };

            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: name.to_string(),
                version: resolved_version,
                arch: None,
                source_path: source_path.to_string(),
                depends: Vec::new(),
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
                extra_annotations: Default::default(),
                binary_role: None,
            });
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Milestone 667 Pass 1: build the lookup tables the m667 Pass 2
    // (edge extraction) consumes. Two indexes:
    //
    // (a) `packages_key_index: HashMap<&str, String>` — maps every
    //     `packages`-map key to the emitted component's
    //     `"<name> <version>"` disambiguation string (matches the
    //     `package_lock.rs:261` convention that the graph builder's
    //     secondary `name_to_purl` key at `scan_fs/mod.rs:635-644`
    //     consumes — see `specs/667-bun-lock-edges/research.md` §R1).
    //
    // (b) `parent_key_to_out_idx: HashMap<&str, usize>` — maps every
    //     packages-map key to the corresponding component's index in
    //     `out`. Enables O(1) mutation lookup during Pass 2's edge-
    //     attachment.
    //
    // This is a fresh iteration over `packages`; the Step 2 emission
    // loop above stays byte-identical to pre-m667 per FR-009. Skip
    // criteria mirror Step 2 (workspace: source-specs + workspace-
    // member name collisions).
    //
    // See `specs/667-bun-lock-edges/contracts/depends-emission.md` C4
    // (multi-version integrity) + data-model.md §PackagesKeysIndex.
    // ────────────────────────────────────────────────────────────────
    let mut packages_key_index: std::collections::HashMap<&str, String> =
        std::collections::HashMap::new();
    let mut parent_key_to_out_idx: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    if let Some(packages) = root.get("packages").and_then(|v| v.as_object()) {
        let mut emit_seq = 0usize;
        for (key, value) in packages {
            let Some(spec) = value
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let Some((name, source_spec)) = spec.rsplit_once('@') else {
                continue;
            };
            if name.is_empty() || source_spec.is_empty() {
                continue;
            }
            // Skip criteria: mirror Step 2's line 223-230 exactly to
            // ensure emit_seq stays in sync with Step 2's emissions.
            if source_spec.starts_with("workspace:") {
                continue;
            }
            if workspace_member_names.contains(name) {
                continue;
            }
            // Override resolution: same as Step 2 line 235-238. Ensures
            // packages_key_index carries the OVERRIDDEN version, so edges
            // routed via Pass 2 target the overridden-version PURL
            // automatically (contract C9).
            let resolved_version = overrides
                .get(name)
                .cloned()
                .unwrap_or_else(|| source_spec.to_string());
            // Validate the PURL builds; if it doesn't, Step 2 would have
            // also skipped this entry — keep emit_seq in sync by NOT
            // incrementing when we skip.
            if build_npm_purl(name, &resolved_version).is_none() {
                continue;
            }
            let disambiguator = format!("{} {}", name, resolved_version);
            packages_key_index.insert(key.as_str(), disambiguator);
            parent_key_to_out_idx.insert(key.as_str(), step2_start + emit_seq);
            emit_seq += 1;
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Milestone 667 Pass 2 + finalize tracking (T010): walk each
    // parent's metadata object at tuple position [2], resolve each
    // dep-name via `resolve_bun_key`, and populate the parent's
    // `depends` field with `"<name> <version>"` disambiguation strings
    // that the graph builder's secondary `name_to_purl` key at
    // `scan_fs/mod.rs:635-644` consumes.
    //
    // Also accumulates per-TARGET optionality tracking into
    // `target_opt_state` for the m667 finalize pass (T012) to apply
    // `LifecycleScope::Optional` + `waybill:optional-derivation`
    // annotation per data-model V6 / contract C7.
    //
    // FR-011 warn-and-drop logging on every dropped edge lands in
    // T011 (added to the `continue` sites below in a follow-on task).
    //
    // See `specs/667-bun-lock-edges/contracts/depends-emission.md`
    // C1 (edge-source completeness) + C3 (`<name> <version>` format)
    // + C4 (multi-version integrity).
    // ────────────────────────────────────────────────────────────────
    #[derive(Default)]
    struct TargetOptionalityState {
        any_hard: bool,
        seen_optional_deps: bool,
        seen_optional_peers: bool,
    }
    let mut target_opt_state: std::collections::HashMap<
        String,
        TargetOptionalityState,
    > = std::collections::HashMap::new();

    // Materialize the resolver's search-space once from Pass 1's index.
    let packages_keys: std::collections::HashSet<&str> =
        packages_key_index.keys().copied().collect();

    if let Some(packages) = root.get("packages").and_then(|v| v.as_object()) {
        for (parent_key, value) in packages {
            // Resolve tuple position [2] — the metadata object.
            let metadata = match value.as_array().and_then(|a| a.get(2)) {
                Some(serde_json::Value::Object(m)) => m,
                Some(_) => {
                    tracing::warn!(
                        parent = %parent_key,
                        reason = "metadata_malformed",
                        "bun.lock edge dropped"
                    );
                    continue;
                }
                None => {
                    tracing::warn!(
                        parent = %parent_key,
                        reason = "metadata_absent",
                        "bun.lock edge dropped"
                    );
                    continue;
                }
            };

            // Locate the parent in `out`. Skips (workspace shape, no
            // valid PURL) are silent because Pass 1's same-criteria
            // filter already excluded the parent from
            // `parent_key_to_out_idx` — Pass 2 must skip too to
            // preserve identity between Pass 1's index and the loop.
            let Some(&parent_out_idx) = parent_key_to_out_idx.get(parent_key.as_str()) else {
                continue;
            };

            // Per-parent edge accumulation. BTreeMap gives deterministic
            // dep-name-sorted output + free dedup when the same dep-name
            // appears in multiple sections (matches
            // `package_lock.rs:181-285` precedent).
            let mut depends_set: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();

            for section in &[
                "dependencies",
                "peerDependencies",
                "optionalDependencies",
                "optionalPeers",
            ] {
                let Some(deps_map) = metadata.get(*section).and_then(|v| v.as_object()) else {
                    continue;
                };
                for (dep_name, range_value) in deps_map {
                    // Empty / null range = malformed lockfile line;
                    // drop the edge but keep parsing (FR-011).
                    match range_value.as_str() {
                        Some(s) if !s.is_empty() => (),
                        _ => {
                            tracing::warn!(
                                parent = %parent_key,
                                dep = %dep_name,
                                reason = "empty_range",
                                "bun.lock edge dropped"
                            );
                            continue;
                        }
                    };

                    let Some(resolved_key) = resolve_bun_key(
                        parent_key.as_str(),
                        dep_name,
                        &packages_keys,
                    ) else {
                        tracing::warn!(
                            parent = %parent_key,
                            dep = %dep_name,
                            reason = "unresolved",
                            "bun.lock edge dropped"
                        );
                        continue;
                    };

                    // Defensive: resolver returned Some, so the key IS
                    // in packages_keys, so it must be in packages_key_index
                    // (they're built from the same source). If this misses,
                    // the invariant broke somewhere upstream.
                    let Some(disambiguator) = packages_key_index.get(resolved_key.as_str())
                    else {
                        continue;
                    };

                    // Insert dedup'd. Every disambiguator this reader
                    // produces is `"<name> <version>"`-shaped (never bare
                    // name), so entry-or-insert is sufficient — no need
                    // for the `package_lock.rs:273-279` version-pinned-
                    // wins precedence check.
                    depends_set
                        .entry(dep_name.clone())
                        .or_insert_with(|| disambiguator.clone());

                    // Update per-target optionality tracking. Uses
                    // resolved_key as the tracking key (matches the
                    // packages-map key), so T012's finalize pass can
                    // look up the target's `out` index via
                    // `parent_key_to_out_idx[resolved_key]`.
                    let entry = target_opt_state
                        .entry(resolved_key.clone())
                        .or_default();
                    match *section {
                        "dependencies" | "peerDependencies" => entry.any_hard = true,
                        "optionalDependencies" => entry.seen_optional_deps = true,
                        "optionalPeers" => entry.seen_optional_peers = true,
                        _ => unreachable!(),
                    }
                }
            }

            // Attach the parent's edges. Deterministic order via BTreeMap.
            out[parent_out_idx].depends = depends_set.into_values().collect();
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Milestone 667 finalize pass (T012): apply optional-scope tagging
    // to targets reached EXCLUSIVELY via optional / optional-peers
    // sections. Per data-model V5 (hard beats optional) + V6 (optional-
    // deps derivation string wins over optional-peers) + contract C7.
    //
    // Tag shape (matches m180 package_lock.rs precedent):
    // - Target's `lifecycle_scope` field → `Some(LifecycleScope::Optional)`.
    // - Target's `extra_annotations["waybill:optional-derivation"]` →
    //   `"bun-optional-dependencies"` (if seen via optionalDependencies)
    //   OR `"bun-optional-peers"` (if seen ONLY via optionalPeers).
    //
    // Downstream m179 emission machinery (`scan_fs/mod.rs:897` +
    // `waybill_common::resolution::RelationshipType`) picks up the
    // Optional lifecycle scope on the target and emits
    // `RelationshipType::OptionalDependsOn` for each incoming edge —
    // which the CDX emitter then renders as `scope: "optional"` at
    // the component level, and SPDX 2.3 as `OPTIONAL_DEPENDENCY_OF`.
    // ────────────────────────────────────────────────────────────────
    {
        use waybill_common::resolution::LifecycleScope;
        for (target_key, state) in &target_opt_state {
            if state.any_hard {
                // Data-model V5: hard edge wins; leave target untagged.
                continue;
            }
            if !state.seen_optional_deps && !state.seen_optional_peers {
                // Impossible per Pass 2's tracker logic, but defensive.
                continue;
            }
            let Some(&target_out_idx) =
                parent_key_to_out_idx.get(target_key.as_str())
            else {
                // Target isn't in `out` (shouldn't happen — target_key
                // came from packages_key_index which is a subset of
                // parent_key_to_out_idx). Defensive skip.
                continue;
            };
            let target = &mut out[target_out_idx];
            target.lifecycle_scope = Some(LifecycleScope::Optional);
            // V6: -dependencies wins when both sections referenced the
            // same target with no hard edge. Only `-peers` if EXCLUSIVELY
            // via optional-peers.
            let derivation = if state.seen_optional_deps {
                "bun-optional-dependencies"
            } else {
                "bun-optional-peers"
            };
            target.extra_annotations.insert(
                "waybill:optional-derivation".to_string(),
                serde_json::Value::String(derivation.to_string()),
            );
        }
    }

    // Step 3: synthesize the workspace-root component when a
    // workspace was detected. Workspace-root's `depends` lists each
    // member by name, producing dependsOn edges to each member in
    // the emitted SBOM (per FR-015).
    if !workspace_member_names.is_empty() {
        let root_pkg_json = rootfs.join("package.json");
        let root_name = workspace_root_name(workspace_root_name_field.as_deref());
        if let Some(mut root_entry) = synthesize_workspace_root(&root_name, &root_pkg_json) {
            root_entry.depends = workspace_member_names.iter().cloned().collect();
            out.push(root_entry);
        }
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T014: resolver walk order — "most specific first"
    //
    // Proves the R2 scope-aware walker for parent
    // `"@fast-csv/format/@types/node"` walks in this exact order:
    //   1. "@fast-csv/format/@types/node/tslib"  (all 2 segments + dep)
    //   2. "@fast-csv/format/tslib"              (segment 0 + dep)
    //   3. "tslib"                                (bare/hoisted)
    //
    // For each candidate position, seed `packages_keys` with ONLY
    // that key and assert `resolve_bun_key` returns it — proving the
    // walker actually tries that position before falling through.
    // A control with an empty packages_keys asserts None.
    //
    // Note: research.md §R2's first test vector originally listed an
    // extra intermediate `"@fast-csv/format/@types/tslib"` candidate.
    // That was a scope-boundary miscount at spec-authoring time; the
    // implementation follows the corrected 3-candidate walk per the
    // T007 rustdoc.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn resolve_bun_key_walks_scope_prefix_most_specific_first() {
        let parent = "@fast-csv/format/@types/node";
        let dep = "tslib";

        // Position 1: most-specific — full parent path prefix.
        let mut keys = std::collections::HashSet::new();
        keys.insert("@fast-csv/format/@types/node/tslib");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("@fast-csv/format/@types/node/tslib".to_string()),
            "walker MUST return the most-specific candidate when it exists",
        );

        // Position 2: mid-level — after peeling `@types/node` segment.
        // Proves the walker tries this level (not just the two ends).
        let mut keys = std::collections::HashSet::new();
        keys.insert("@fast-csv/format/tslib");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("@fast-csv/format/tslib".to_string()),
            "walker MUST fall through to mid-level candidate when more-specific misses",
        );

        // Position 3: root/hoisted — the bare dep name.
        let mut keys = std::collections::HashSet::new();
        keys.insert("tslib");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("tslib".to_string()),
            "walker MUST fall through to bare-name (root-hoisted) candidate",
        );

        // Control: empty packages_keys → None regardless of parent shape.
        let empty = std::collections::HashSet::new();
        assert_eq!(
            resolve_bun_key(parent, dep, &empty),
            None,
            "walker MUST return None when NO candidate matches",
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T015: resolver handles scoped-name dep_name
    //
    // When the DEP_NAME itself is scoped (e.g. `@scope/pkg`), the
    // walker must still walk parent's prefix from most-specific to
    // root, appending the scoped dep-name at each level:
    //   1. "foo/bar/baz/@scope/pkg"
    //   2. "foo/bar/@scope/pkg"
    //   3. "foo/@scope/pkg"
    //   4. "@scope/pkg"
    // (4 candidates for a 3-segment non-scoped parent.)
    // ────────────────────────────────────────────────────────────
    #[test]
    fn resolve_bun_key_handles_scoped_dep_name() {
        let parent = "foo/bar/baz";
        let dep = "@scope/pkg";

        // Position 2 (out of 4): mid-level match.
        let mut keys = std::collections::HashSet::new();
        keys.insert("foo/bar/@scope/pkg");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("foo/bar/@scope/pkg".to_string()),
            "walker MUST correctly append scoped dep-name at each prefix level",
        );

        // Root/hoisted with scoped dep name.
        let mut keys = std::collections::HashSet::new();
        keys.insert("@scope/pkg");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("@scope/pkg".to_string()),
            "walker MUST reach root-hoisted scoped dep-name",
        );

        // Most-specific position (all 3 parent segments + dep).
        let mut keys = std::collections::HashSet::new();
        keys.insert("foo/bar/baz/@scope/pkg");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("foo/bar/baz/@scope/pkg".to_string()),
            "walker MUST try the most-specific position for scoped dep-name",
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T016: resolver on hoisted (1-segment) parent
    //
    // For a root-hoisted parent, the walk has only 2 candidates:
    //   1. "lodash/chalk"  (parent prefix + dep)
    //   2. "chalk"          (bare / hoisted)
    // ────────────────────────────────────────────────────────────
    #[test]
    fn resolve_bun_key_hoisted_parent() {
        let parent = "lodash";
        let dep = "chalk";

        // Position 1: mid-level match under lodash's namespace.
        let mut keys = std::collections::HashSet::new();
        keys.insert("lodash/chalk");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("lodash/chalk".to_string()),
            "walker MUST try parent-prefix candidate for hoisted parent",
        );

        // Position 2: hoisted (bare-name) match.
        let mut keys = std::collections::HashSet::new();
        keys.insert("chalk");
        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            Some("chalk".to_string()),
            "walker MUST fall through to bare-name for hoisted parent",
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T017: resolver returns None on complete miss
    //
    // Distinct from T014's empty-keys control: T017 seeds a
    // non-empty packages_keys that has NO overlap with any walk
    // position. Proves the walker exhausts its candidates and
    // returns None rather than silently returning a bogus key.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn resolve_bun_key_returns_none_on_complete_miss() {
        let parent = "foo/bar/baz";
        let dep = "target-dep";

        // packages_keys is populated but none match ANY walk position.
        let mut keys = std::collections::HashSet::new();
        keys.insert("unrelated-key");
        keys.insert("another/unrelated");
        keys.insert("@scope/thing");
        // Also add near-misses (same shape but different names) to
        // guard against off-by-one prefix comparisons.
        keys.insert("foo/bar/OTHER-DEP");
        keys.insert("target-dep-suffix");

        assert_eq!(
            resolve_bun_key(parent, dep, &keys),
            None,
            "walker MUST return None when candidates exhaust with no hit",
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T018: reader-level end-to-end minimal repro
    //
    // Fixture: `tests/fixtures/bun_lock/minimal_repro/bun.lock`
    // (issue #723's exact 2-file reproduction). Loaded via
    // include_str! so the on-disk fixture stays the single source of
    // truth verified by both unit + integration tests.
    //
    // Post-fix invariants:
    //   - 2 registry entries emitted (parent-pkg + child-pkg). No
    //     workspace-root synth: the workspaces map has only key ""
    //     with no non-root members, matching the pre-fix
    //     `emits_basic_npm_components` shape.
    //   - parent-pkg.depends contains `"child-pkg 1.0.0"` — the
    //     `<name> <version>` disambiguation string per R1, matching
    //     the `package_lock.rs:261` convention consumed by the
    //     graph builder's secondary `name_to_purl` key at
    //     `scan_fs/mod.rs:635-644`.
    //   - child-pkg.depends is empty (leaf).
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_emits_transitive_edges() {
        let src = include_str!(
            "../../../../tests/fixtures/bun_lock/minimal_repro/bun.lock"
        );
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        assert_eq!(entries.len(), 2, "got: {entries:?}");
        let parent = entries.iter().find(|e| e.name == "parent-pkg").unwrap();
        let child = entries.iter().find(|e| e.name == "child-pkg").unwrap();

        assert!(
            parent.depends.iter().any(|d| d == "child-pkg 1.0.0"),
            "parent-pkg.depends MUST contain the child-pkg edge in \
             `<name> <version>` shape; got: {:?}",
            parent.depends,
        );
        assert!(
            child.depends.is_empty(),
            "child-pkg is a leaf; depends MUST be empty; got: {:?}",
            child.depends,
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T019: multi-version disambiguation (SC-004)
    //
    // Fixture: `tests/fixtures/bun_lock/multi_version/bun.lock`
    // Encodes `minimatch` at two versions under two different
    // parents (`big/minimatch` → 3.1.2, `small/minimatch` → 5.1.6).
    // Bun's non-hoisted linker produces this exact shape when a
    // shared transitive resolves to two different versions per
    // caller's semver range.
    //
    // Post-fix invariant: each parent's edge points at the CORRECT
    // version copy — big → 3.1.2 (NOT 5.1.6), small → 5.1.6 (NOT
    // 3.1.2). Correctness depends on R1's `<name> <version>` disambiguation
    // string (a bare `"minimatch"` would collide) and the resolver's
    // C4 multi-version integrity.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_multi_version_disambiguation() {
        let src = include_str!(
            "../../../../tests/fixtures/bun_lock/multi_version/bun.lock"
        );
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        // 4 registry entries: big + 2 minimatch versions + small.
        assert_eq!(entries.len(), 4, "got: {entries:?}");

        // Both minimatch versions emit as distinct entries.
        let mm_versions: Vec<&str> = entries
            .iter()
            .filter(|e| e.name == "minimatch")
            .map(|e| e.version.as_str())
            .collect();
        assert!(
            mm_versions.contains(&"3.1.2") && mm_versions.contains(&"5.1.6"),
            "both minimatch versions MUST be emitted; got: {mm_versions:?}",
        );

        let big = entries.iter().find(|e| e.name == "big").unwrap();
        assert!(
            big.depends.iter().any(|d| d == "minimatch 3.1.2"),
            "big@1.0.0.depends MUST contain minimatch 3.1.2 (NOT 5.1.6); \
             got: {:?}",
            big.depends,
        );
        assert!(
            !big.depends.iter().any(|d| d == "minimatch 5.1.6"),
            "big@1.0.0.depends MUST NOT contain the 5.1.6 version; got: {:?}",
            big.depends,
        );

        let small = entries.iter().find(|e| e.name == "small").unwrap();
        assert!(
            small.depends.iter().any(|d| d == "minimatch 5.1.6"),
            "small@2.0.0.depends MUST contain minimatch 5.1.6 (NOT 3.1.2); \
             got: {:?}",
            small.depends,
        );
        assert!(
            !small.depends.iter().any(|d| d == "minimatch 3.1.2"),
            "small@2.0.0.depends MUST NOT contain the 3.1.2 version; got: {:?}",
            small.depends,
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T020: scoped-name resolver (SC-005)
    //
    // Fixture: `tests/fixtures/bun_lock/scoped_name/bun.lock`
    // Parent key `@fast-csv/format` (2-segment scope path) has a
    // scope-nested `@types/node` at `@fast-csv/format/@types/node`.
    // The R2 walker's scope-atomic segmenter MUST peel the scope
    // pair `@fast-csv/format` as one segment and correctly find the
    // scope-nested target.
    //
    // Post-fix invariant: `@fast-csv/format`.depends contains
    // `"@types/node 22.5.0"` (from the scope-nested key), not any
    // root-hoisted `@types/node` (there isn't one in this fixture,
    // but the invariant holds regardless).
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_scoped_name_resolver() {
        let src = include_str!(
            "../../../../tests/fixtures/bun_lock/scoped_name/bun.lock"
        );
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        assert_eq!(entries.len(), 2, "got: {entries:?}");
        let fast_csv = entries
            .iter()
            .find(|e| e.name == "@fast-csv/format")
            .expect("scope-name parent must be emitted");
        assert!(
            fast_csv.depends.iter().any(|d| d == "@types/node 22.5.0"),
            "@fast-csv/format.depends MUST contain `@types/node 22.5.0` \
             (the scope-nested version); got: {:?}",
            fast_csv.depends,
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T021: optional-derivation tagging (US1 scenario 3)
    //
    // Fixture: `tests/fixtures/bun_lock/optional_deps/bun.lock`
    // Parent declares `opt-child` via `optionalDependencies` only —
    // no hard section references it. Data-model V6 mandates:
    //   - target.lifecycle_scope = Some(LifecycleScope::Optional)
    //   - target.extra_annotations["waybill:optional-derivation"] =
    //       "bun-optional-dependencies"
    //   - parent.depends still contains the edge string (the target-
    //     side scope tag, not the edge itself, carries the
    //     optionality per m180 convention).
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_optional_dep_tagging() {
        use waybill_common::resolution::LifecycleScope;

        let src = include_str!(
            "../../../../tests/fixtures/bun_lock/optional_deps/bun.lock"
        );
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        assert_eq!(entries.len(), 2, "got: {entries:?}");

        let parent = entries.iter().find(|e| e.name == "parent").unwrap();
        assert!(
            parent.depends.iter().any(|d| d == "opt-child 1.0.0"),
            "parent.depends MUST still contain the opt-child edge; got: {:?}",
            parent.depends,
        );

        let opt_child = entries.iter().find(|e| e.name == "opt-child").unwrap();
        assert_eq!(
            opt_child.lifecycle_scope,
            Some(LifecycleScope::Optional),
            "opt-child.lifecycle_scope MUST be Some(Optional); got: {:?}",
            opt_child.lifecycle_scope,
        );
        assert_eq!(
            opt_child
                .extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("bun-optional-dependencies"),
            "opt-child MUST carry the `bun-optional-dependencies` \
             derivation tag; got: {:?}",
            opt_child.extra_annotations.get("waybill:optional-derivation"),
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T022: hard edge beats optional tagging (C7 + V5)
    //
    // Fixture (inline): `shared` is reached from `parent-a` via a
    // HARD `dependencies` edge AND from `parent-b` via an
    // `optionalDependencies` edge. Per C7 the target MUST NOT be
    // tagged optional — hard wins.
    //
    // The reader walks the packages map in serde_json object-order
    // (insertion order for `serde_json::Value` w/ preserve_order not
    // enabled — but the `target_opt_state.any_hard` flag makes the
    // outcome order-independent). We deliberately author the fixture
    // with the optional-referencing parent FIRST to prove
    // order-independence of the hard-wins rule.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_hard_edge_beats_optional_tag() {
        let src = r#"{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "hard-beats-opt" } },
  "packages": {
    "parent-b": ["parent-b@1.0.0", "", { "optionalDependencies": { "shared": "^1.0.0" } }, "sha512-BBB"],
    "parent-a": ["parent-a@1.0.0", "", { "dependencies": { "shared": "^1.0.0" } }, "sha512-AAA"],
    "shared": ["shared@1.0.0", "", {}, "sha512-SSS"]
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        // Both parents' edges land, both point at shared 1.0.0.
        let a = entries.iter().find(|e| e.name == "parent-a").unwrap();
        let b = entries.iter().find(|e| e.name == "parent-b").unwrap();
        assert!(
            a.depends.iter().any(|d| d == "shared 1.0.0"),
            "parent-a hard edge to shared MUST land; got: {:?}",
            a.depends,
        );
        assert!(
            b.depends.iter().any(|d| d == "shared 1.0.0"),
            "parent-b optional edge to shared MUST land; got: {:?}",
            b.depends,
        );

        // C7: shared is reached via a hard edge, so it MUST NOT be
        // tagged optional even though the optional-referencing parent
        // was walked first.
        let shared = entries.iter().find(|e| e.name == "shared").unwrap();
        assert_eq!(
            shared.lifecycle_scope, None,
            "shared MUST NOT be tagged Optional when any hard edge \
             reaches it (C7 hard-wins); got: {:?}",
            shared.lifecycle_scope,
        );
        assert!(
            !shared
                .extra_annotations
                .contains_key("waybill:optional-derivation"),
            "shared MUST NOT carry a waybill:optional-derivation \
             annotation when reached via a hard edge; got annotations: {:?}",
            shared.extra_annotations,
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T023: warn-and-drop on unresolved dep (C5 + FR-011)
    //
    // Fixture (inline): parent's metadata declares a `missing-dep`
    // name that has NO matching `packages`-map key at any walk
    // position. Per C5, the reader MUST:
    //   - complete successfully (no early return)
    //   - drop the edge (parent.depends stays empty for that dep)
    //   - emit exactly ONE `tracing::warn!` line with reason=unresolved
    //
    // We assert (a) + (b) structurally. Assertion (c) — the warn-log
    // shape — is left to observer inspection: the test doesn't wire
    // in a `tracing_test` subscriber (would add a dev-dep). The
    // structural assertion in (b) is the actual invariant; the warn
    // is a diagnostic side-channel.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_warn_and_drop_on_unresolved() {
        let src = r#"{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "unresolved-drop" } },
  "packages": {
    "parent": ["parent@1.0.0", "", { "dependencies": { "missing-dep": "^1.0.0" } }, "sha512-PPP"]
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        // (a) Reader completes successfully (this call would panic on
        // any early-return of a Result; parse_bun_lock returns Vec).
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        // 1 registry entry (parent); the unresolved dep does NOT
        // become a phantom component (Constitution Principle XII).
        assert_eq!(entries.len(), 1, "got: {entries:?}");

        // (b) parent.depends MUST NOT contain the unresolved name.
        let parent = entries.iter().find(|e| e.name == "parent").unwrap();
        assert!(
            !parent.depends.iter().any(|d| d.starts_with("missing-dep")),
            "parent.depends MUST NOT contain any entry for the \
             unresolved dep name; got: {:?}",
            parent.depends,
        );
        assert!(
            parent.depends.is_empty(),
            "parent.depends MUST be empty when its only declared dep \
             is unresolved; got: {:?}",
            parent.depends,
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T024: component-set preservation (FR-008 + C6)
    //
    // Same minimal-repro fixture as T018, but this test focuses
    // ONLY on the count invariant: adding Pass 2 (edge extraction)
    // MUST NOT change how many components the reader emits vs the
    // pre-fix behavior. The pre-fix reader emitted 2 registry
    // entries for this fixture (parent-pkg + child-pkg — both from
    // Step 2's packages-map walk; no workspace-root synthesis since
    // the workspaces map has only key "" with no members). Post-fix
    // MUST also emit exactly 2.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_preserves_component_count_vs_pre_fix() {
        let src = include_str!(
            "../../../../tests/fixtures/bun_lock/minimal_repro/bun.lock"
        );
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());

        // C6: components.len() invariant across the fix. The pre-fix
        // baseline is 2 (parent-pkg + child-pkg) for this fixture; a
        // regression in component emission would show up as any
        // other number here.
        assert_eq!(
            entries.len(),
            2,
            "component-set invariant violated: pre-fix reader emits 2 \
             for minimal_repro; post-fix MUST also emit 2; got: {entries:?}",
        );
    }

    // ────────────────────────────────────────────────────────────
    // Milestone 667 T025: workspace edges unchanged (FR-007 + C8)
    //
    // Fixture (inline): a 2-member workspace with `workspace:*`
    // intra-workspace edges but NO packages-map entries beyond the
    // workspace shims. Verifies:
    //   - Pass 2's packages walk does NOT touch workspace member
    //     entries (identified via component-role: main-module OR
    //     presence in workspace_member_names).
    //   - Workspace-member's `depends` are populated ONLY from the
    //     workspace: source-spec walker (`bun_lock.rs:135-148`), not
    //     from the new Pass 2 code.
    //   - Workspace-root's `depends` = each member name.
    //
    // Author: a lightweight 2-member workspace where `@my/web`
    // workspace-depends on `@my/shared`. Assert both members carry
    // the m147 workspace shape verbatim.
    // ────────────────────────────────────────────────────────────
    #[test]
    fn parse_bun_lock_workspace_edges_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        write(
            &rootfs.join("package.json"),
            r#"{ "name": "ws-only", "workspaces": ["packages/*"] }"#,
        );
        write(
            &rootfs.join("packages/web/package.json"),
            r#"{ "name": "@my/web", "version": "1.0.0", "dependencies": { "@my/shared": "workspace:*" } }"#,
        );
        write(
            &rootfs.join("packages/shared/package.json"),
            r#"{ "name": "@my/shared", "version": "0.5.0" }"#,
        );
        let src = r#"{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "ws-only" },
    "packages/web": {
      "name": "@my/web",
      "dependencies": {
        "@my/shared": "workspace:*"
      }
    },
    "packages/shared": {
      "name": "@my/shared"
    }
  },
  "packages": {
    "@my/web": ["@my/web@workspace:packages/web"],
    "@my/shared": ["@my/shared@workspace:packages/shared"]
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(src).unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", rootfs);

        // 2 members + 1 workspace-root shim = 3
        assert_eq!(entries.len(), 3, "got: {entries:?}");

        // C8: workspace member `depends` come EXCLUSIVELY from
        // `workspace:*` walker; Pass 2 must not add anything to them.
        let web = entries.iter().find(|e| e.name == "@my/web").unwrap();
        assert_eq!(
            web.depends,
            vec!["@my/shared".to_string()],
            "@my/web.depends MUST contain ONLY the `@my/shared` bare \
             name from the workspace: walker (not a \
             `<name> <version>` disambiguation string from Pass 2); \
             got: {:?}",
            web.depends,
        );

        let shared = entries.iter().find(|e| e.name == "@my/shared").unwrap();
        assert!(
            shared.depends.is_empty(),
            "@my/shared has no workspace: deps; depends MUST be empty; \
             got: {:?}",
            shared.depends,
        );

        // Workspace-root shim: depends = member names.
        let root = entries
            .iter()
            .find(|e| e.purl.as_str() == "pkg:generic/ws-only")
            .expect("workspace-root synth must be emitted");
        let mut root_depends = root.depends.clone();
        root_depends.sort();
        assert_eq!(
            root_depends,
            vec!["@my/shared".to_string(), "@my/web".to_string()],
            "workspace-root.depends MUST list each member by bare name; \
             got: {:?}",
            root.depends,
        );
    }

    #[test]
    fn emits_basic_npm_components() {
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test-app" } },
  "packages": {
    "lodash": ["lodash@4.17.21", "sha512-aaa"],
    "express": ["express@4.18.2", "sha512-bbb"]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        // No workspace members declared → no workspace-root synthesis.
        // Just the 2 registry packages.
        assert_eq!(entries.len(), 2, "got: {entries:?}");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"lodash"));
        assert!(names.contains(&"express"));
        let lodash = entries.iter().find(|e| e.name == "lodash").unwrap();
        assert_eq!(lodash.purl.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn encodes_scoped_packages() {
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test" } },
  "packages": {
    "@types/node": ["@types/node@22.5.0", "sha512-..."]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        assert_eq!(entries.len(), 1);
        // PURL must URL-encode the `@` in the scope segment per PURL spec.
        assert_eq!(entries[0].purl.as_str(), "pkg:npm/%40types/node@22.5.0");
    }

    #[test]
    fn override_version_wins() {
        // overrides map sets lodash to 4.17.21; the packages-map entry
        // has 4.17.20 (different version). Override wins; un-overridden
        // version is NOT emitted as a separate component.
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test" } },
  "packages": {
    "lodash": ["lodash@4.17.20", "sha512-..."]
  },
  "overrides": {
    "lodash": "4.17.21"
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        assert_eq!(entries.len(), 1, "got: {entries:?}");
        assert_eq!(entries[0].version, "4.17.21");
        assert_eq!(entries[0].purl.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn emits_workspace_shape() {
        // Synthetic Bun workspace: 2 members (@my/web, @my/shared) +
        // 1 external dep (lodash). @my/web depends on @my/shared via
        // workspace:* source-spec. Expected output:
        //   - workspace-root component (pkg:generic/my-monorepo)
        //   - @my/web component (main-module + dependsOn @my/shared)
        //   - @my/shared component (main-module)
        //   - lodash component (no role)
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        write(
            &rootfs.join("package.json"),
            r#"{ "name": "my-monorepo", "workspaces": ["packages/*"] }"#,
        );
        write(
            &rootfs.join("packages/web/package.json"),
            r#"{ "name": "@my/web", "version": "1.0.0", "dependencies": { "@my/shared": "workspace:*", "lodash": "^4.17.21" } }"#,
        );
        write(
            &rootfs.join("packages/shared/package.json"),
            r#"{ "name": "@my/shared", "version": "0.5.0" }"#,
        );
        let lockfile_src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "my-monorepo" },
    "packages/web": {
      "name": "@my/web",
      "dependencies": {
        "@my/shared": "workspace:*",
        "lodash": "^4.17.21"
      }
    },
    "packages/shared": {
      "name": "@my/shared"
    }
  },
  "packages": {
    "lodash": ["lodash@4.17.21", "sha512-..."],
    "@my/web": ["@my/web@workspace:packages/web"],
    "@my/shared": ["@my/shared@workspace:packages/shared"]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(lockfile_src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", rootfs);

        // 2 members + 1 external + 1 synthetic workspace-root = 4
        assert_eq!(entries.len(), 4, "got: {entries:?}");

        let web = entries.iter().find(|e| e.name == "@my/web").unwrap();
        assert_eq!(web.purl.as_str(), "pkg:npm/%40my/web@1.0.0");
        assert_eq!(
            web.extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );
        assert_eq!(web.depends, vec!["@my/shared".to_string()]);

        let shared = entries.iter().find(|e| e.name == "@my/shared").unwrap();
        assert_eq!(shared.purl.as_str(), "pkg:npm/%40my/shared@0.5.0");
        assert_eq!(
            shared
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );

        let lodash = entries.iter().find(|e| e.name == "lodash").unwrap();
        assert!(!lodash
            .extra_annotations
            .contains_key("waybill:component-role"));

        let ws_root = entries
            .iter()
            .find(|e| e.purl.as_str() == "pkg:generic/my-monorepo")
            .expect("workspace-root component must be emitted");
        assert_eq!(
            ws_root
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("workspace-root"),
        );
        let mut depends_sorted = ws_root.depends.clone();
        depends_sorted.sort();
        assert_eq!(
            depends_sorted,
            vec!["@my/shared".to_string(), "@my/web".to_string()]
        );
    }

    #[test]
    fn workspace_member_uses_placeholder_when_no_pkg_json() {
        // Edge: workspace member declared in bun.lock but its
        // package.json is missing/unreadable. Use placeholder
        // "0.0.0" version rather than panicking.
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        let lockfile_src = r#"{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "root" },
    "packages/orphan": { "name": "@my/orphan" }
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(lockfile_src).unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", rootfs);
        let orphan = entries.iter().find(|e| e.name == "@my/orphan").unwrap();
        assert_eq!(orphan.version, "0.0.0");
        assert_eq!(orphan.purl.as_str(), "pkg:npm/%40my/orphan@0.0.0");
    }
}
