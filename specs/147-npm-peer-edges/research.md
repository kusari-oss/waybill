# Research — milestone 147 (npm peerDependencies edge emission + peer-kind annotation)

Phase 0 output. Resolves the three design questions /speckit-clarify flagged for plan-phase resolution.

## §A — Sort order of the `mikebom:peer-edge-targets` PURL array

**Decision**: Sort the PURL strings ALPHABETICALLY before stamping into `extra_annotations`. Matches the milestone-145 precedent established for `mikebom:file-paths`.

**Verification**: `mikebom-cli/src/scan_fs/file_tier/mod.rs` (milestone-145 `mikebom:file-paths` site):

```rust
let mut paths_str: Vec<String> = self.paths.iter()
    .take(FILE_PATHS_CAP)
    .map(|p| p.to_string_lossy().into_owned())
    .collect();
// Keep sort-stable in the emitted property too.
paths_str.sort();
```

The `paths_str.sort()` (alphabetical, lex-ascending) is the established project pattern for array-valued mikebom annotations. Reproducibility for byte-identity goldens requires deterministic ordering; alphabetical is the obvious + consumer-friendly choice.

**Rationale**:
- Byte-identity goldens require deterministic order (insertion order from a `BTreeMap` iteration is already alphabetical-by-key, but we should be explicit and sort the values too).
- Alphabetical lets downstream consumers binary-search the array if needed.
- Matches existing project convention; no new mental model for contributors.

**Alternatives considered**:
- **Insertion order from the lockfile** — rejected: lockfile-section order is unstable across npm versions / package-lock formats.
- **Sort by version after name** — same as alphabetical (PURL strings include `@<version>` suffix, so alpha-sort puts same-name-different-version PURLs in version order automatically).

## §B — Unmet-peer behavior is automatic via existing helper

**Decision**: No additional code needed for FR-002 (unmet peers → no edge). The existing `resolve_dep_via_node_modules_walk` helper at `package_lock.rs:354-381` returns `None` naturally when the dep isn't installed.

**Verification** (from the helper's doc comment at lines 350-353):

> *"Returns `None` if `dep_name` isn't installed at any level. This is rare in well-formed lockfiles but can happen when a dep is declared but not actually resolved (e.g., `optionalDependencies` that failed install)."*

The function walks up the node_modules tree, checking `<prefix>/node_modules/<dep>` at each level via `path_versions.get(...)`. If `path_versions` doesn't contain a `node_modules/<dep>` entry at ANY level (including the top-level hoisted location), the function returns `None`. The caller's `.unwrap_or_else(|| dep_name.clone())` falls back to bare-name, which the downstream edge resolver in `scan_fs/mod.rs` drops when no PURL match is found.

**Implication for milestone 147**: when iterating `peerDependencies` in the new code path, an unmet peer (declared in `peerDependencies` but absent from `packages` map) flows through the same `.unwrap_or_else(|| dep_name.clone())` → bare-name → no-PURL-match → no edge path. The behavior is FREE; we just need to NOT add unmet peers to the `mikebom:peer-edge-targets` array (track them separately via the `resolve_dep_via_node_modules_walk` return value).

**Algorithm sketch** (illustrative; not normative):

```rust
let mut depends_set: BTreeMap<String, String> = BTreeMap::new();
let mut peer_edge_targets: BTreeSet<String> = BTreeSet::new();  // NEW

// Existing 3 sections — unchanged.
for section in &["dependencies", "devDependencies", "optionalDependencies"] {
    if let Some(deps) = tbl.get(*section).and_then(|v| v.as_object()) {
        for dep_name in deps.keys() {
            let resolved = resolve_dep_via_node_modules_walk(path_key, dep_name, &path_versions)
                .map(|version| format!("{dep_name} {version}"))
                .unwrap_or_else(|| dep_name.clone());
            // ... existing BTreeMap entry handling ...
        }
    }
}

// NEW: peerDependencies section.
if let Some(peer_deps) = tbl.get("peerDependencies").and_then(|v| v.as_object()) {
    for dep_name in peer_deps.keys() {
        // Skip if already in depends_set via a regular section (FR-003).
        if depends_set.contains_key(dep_name) {
            continue;
        }
        // Resolve; emit edge only when peer is installed (FR-002).
        if let Some(version) = resolve_dep_via_node_modules_walk(path_key, dep_name, &path_versions) {
            let resolved = format!("{dep_name} {version}");
            depends_set.insert(dep_name.clone(), resolved.clone());
            // Build the PURL string for the annotation (peer-edge-targets uses PURL, not the
            // "<name> <version>" form which is internal to the dep-graph resolver).
            let purl_str = build_npm_purl(dep_name, version);
            peer_edge_targets.insert(purl_str);
        }
        // Unmet peer (resolve returned None) → no edge, no annotation entry.
    }
}

// Stamp the annotation only when non-empty (FR-005).
if !peer_edge_targets.is_empty() {
    let sorted_arr: Vec<serde_json::Value> = peer_edge_targets
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    extra_annotations.insert(
        "mikebom:peer-edge-targets".to_string(),
        serde_json::Value::Array(sorted_arr),
    );
}
```

**Note on BTreeSet vs Vec+sort**: `BTreeSet<String>` gives us sort + dedupe for free; converting via `.into_iter()` yields alphabetically-sorted PURL strings. Equivalent to milestone 145's `paths_str.sort()` pattern at the type level.

## §C — Golden fixture audit

**Decision**: 3 npm-bearing fixtures in scope. The `.actual.json` siblings are debug artifacts (test-runtime emissions for failed comparison) and don't need refresh — they're regenerated automatically on test runs.

**Verification**: `grep -rln 'pkg:npm/' mikebom-cli/tests/fixtures/golden/`:

```
mikebom-cli/tests/fixtures/golden/spdx-3/npm.spdx3.json
mikebom-cli/tests/fixtures/golden/spdx-3/npm.spdx3.actual.json     # debug-only
mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.json
mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.actual.json    # debug-only
mikebom-cli/tests/fixtures/golden/spdx-2.3/npm.spdx.json
mikebom-cli/tests/fixtures/golden/spdx-2.3/npm.spdx.actual.json    # debug-only
```

Three refresh targets:
- `cyclonedx/npm.cdx.json`
- `spdx-2.3/npm.spdx.json`
- `spdx-3/npm.spdx3.json`

**Expected diff scope per file**:
- Net new `dependsOn` (CDX) / `DEPENDS_ON` (SPDX 2.3) / relationship element (SPDX 3) entries for peer-driven edges in the npm fixture's component graph.
- Net new `mikebom:peer-edge-targets` properties (CDX) / envelope annotations (SPDX 2.3 + SPDX 3) on components that own peer-driven edges.

**Sizing**: The npm fixture is a small synthetic project; the peer-driven edge count is bounded by however many peers the fixture's lockfile happens to declare. Likely 0-2 per file. If the fixture lockfile contains NO `peerDependencies` declarations, the goldens are unchanged (and we should consider extending the fixture to cover the new code path).

**Action**: Phase 5 tasks (golden audit + refresh) will run the refresh trifecta + inspect via `git diff --stat`. If the diff is empty (fixture doesn't exercise peer-edges), tasks.md will include a sub-task to extend the npm fixture's `package-lock.json` to include a peer-dependency case (Yocto-style minimal — e.g., `mlly` declaring `pathe` as peer, mirroring the existing reader unit test at `package_lock.rs:680-711`).

## §D — Test strategy

**Decision**: 4 new in-file unit tests in `package_lock.rs#mod tests` covering FR-001..FR-005, replacing the existing `peer_dependencies_are_skipped_declarative_not_install` test (per FR-007). No new out-of-source integration test needed — the existing reader tests + the parity-catalog row addition cover the cross-format invariance.

**Unit tests**:
1. `peer_dependencies_emit_edges_and_annotation_md147` (replaces the pre-147 skip test) — `mlly` declares `pathe` as peer; assert (a) `mlly.depends` contains `pathe@2.0.3`, (b) `mlly.extra_annotations.get("mikebom:peer-edge-targets")` is `Some(Value::Array([Value::String("pkg:npm/pathe@2.0.3")]))`. Covers FR-001 + FR-004 + SC-003.
2. `peer_already_in_regular_deps_takes_precedence_md147` — package X declares Y in both `peerDependencies` and `dependencies`; assert (a) X has ONE edge to Y, (b) X has NO `mikebom:peer-edge-targets` annotation. Covers FR-003 + SC-004.
3. `unmet_peer_emits_no_edge_md147` — package X declares Y as peer; Y NOT in lockfile's `packages` map; assert (a) X has no edge to Y, (b) X has no `mikebom:peer-edge-targets` annotation. Covers FR-002 + SC-005.
4. `peer_annotation_omitted_when_set_empty_md147` — package X has zero peer-driven edges (only regular deps); assert X has NO `mikebom:peer-edge-targets` key in `extra_annotations` (omitted, not empty-array). Covers FR-005.
5. `peer_edge_targets_array_is_sorted_alphabetically_md147` — package X declares multiple peers (e.g., `react`, `lodash`, `axios`); assert the annotation value is `["pkg:npm/axios@1.0.0", "pkg:npm/lodash@4.0.0", "pkg:npm/react@17.0.0"]` (alphabetical). Covers §A sort precedent.

**Parity catalog row** (per SC-002): add a new row to `mikebom-cli/src/parity/extractors/mod.rs` (next available C-number, likely C97 — verify in tasks.md) with `Directionality::SymmetricEqual` and `order_sensitive: false` (the array IS sorted alphabetically per §A, so order is implicitly consistent; SymmetricEqual is the right invariant).

## §E — Comment-text update mandatory

The current comment at `package_lock.rs:168-176` says:

> *"Skip peerDependencies — semantically declarative ('the consumer should have X installed'), not an install relationship. npm v7+ auto-installs peers as a convenience, but the SBOM dependsOn / DEPENDS_ON slot means 'X depends on Y' not 'X expects Y to be present.' Trivy and syft also skip peer-edges."*

This comment is internally inconsistent (lines 149-160 above already document the intent to "Walk ALL four standard npm dep sections — dependencies, devDependencies, peerDependencies, optionalDependencies") AND it's factually wrong about Trivy (the Trivy comparison surfaced in the audit shows Trivy DOES emit peer-edges).

**Decision**: Rewrite the comment to reflect the milestone-147 policy:

```rust
// Walk all four standard npm dep sections. peerDependencies were
// historically skipped (matching Syft's behavior pre-147), but
// milestone 147 enables them to close the orphan gap surfaced by
// the Trivy comparison on the looker-frontend lockfile (Trivy
// emits peer-edges as DEPENDS_ON; 5 mikebom orphans dropped to 0
// matching Trivy).
//
// The install-vs-functional distinction is preserved via a
// mikebom:peer-edge-targets annotation on the source component
// listing the PURLs of peer-driven edges (Constitution Principle V
// parity-bridging — CDX/SPDX 2.3/SPDX 3 all lack a native carrier
// for per-edge peer-kind metadata). Documented in
// docs/reference/sbom-format-mapping.md.
//
// FR-002 (no phantom edges for unmet peers) is satisfied for free
// via resolve_dep_via_node_modules_walk returning None when the
// peer isn't installed at any level.
```

**Rationale**: Future contributors landing on this code MUST see the current policy reflected; leaving the stale "Trivy and syft also skip" rationale would surprise + mislead. The new comment also points at the spec milestone for context.

## Summary of decisions feeding Phase 1

- **§A**: Alphabetical sort via `BTreeSet<String>` for the `mikebom:peer-edge-targets` annotation value. Matches milestone-145 `mikebom:file-paths` precedent.
- **§B**: Unmet-peer behavior is automatic via existing `resolve_dep_via_node_modules_walk` helper. No new code needed for FR-002.
- **§C**: 3 golden fixtures in scope (cyclonedx + spdx-2.3 + spdx-3 npm.*.json). May need fixture extension if existing lockfile lacks peer-deps.
- **§D**: 5 unit tests + 1 parity-catalog row.
- **§E**: Comment-text rewrite mandatory; the existing comment is internally inconsistent + factually wrong about Trivy.
- **No new Cargo dependencies.**
