# Research: bun.lock transitive-edge emission — Phase 0 outputs

## R1: `depends`-field convention (correction to spec Q1)

**Decision**: Populate `PackageDbEntry.depends` with **`"<name> <version>"` disambiguation strings** matching the exact convention `package_lock.rs:261` uses today. Bare-name fallback (single string with no space) permitted for entries where the resolver returns `None` for a non-required edge — but for THIS reader, every resolved edge will produce a space-separated `<name> <version>` string because bun's lockfile always pins a specific version at each key path.

**Rationale**: The spec's Q1 clarification (2026-08-27) said "populate `depends` with target PURLs directly." This was a **misreading of the `package_lock.rs:285` line by me at spec-authoring time**. Actual convention verified during Phase 0 code inspection:

```rust
// package_lock.rs:261
let resolved = resolve_dep_via_node_modules_walk(path_key, dep_name, &path_versions)
    .map(|version| format!("{dep_name} {version}"))  // ← space-separated <name> <version>
    .unwrap_or_else(|| dep_name.clone());              // ← bare name fallback
// ...
let depends: Vec<String> = depends_set.into_values().collect();
```

The graph builder at `scan_fs/mod.rs:635-644` (m087 for cargo, m147 issue #262 extension to npm/golang) explicitly builds a **secondary `name_to_purl` key** for this shape:

```rust
if ecosystem == "cargo" || ecosystem == "npm" || ecosystem == "golang" {
    let nv_key = format!("{} {}", e.name, e.version);   // ← "name version" secondary key
    name_to_purl.insert(
        (ecosystem, normalize_dep_name(e.purl.ecosystem(), &nv_key)),
        e.purl.as_str().to_string(),
    );
}
```

Then at edge-emission time (`scan_fs/mod.rs:897-899`):

```rust
for dep_name in &entry.depends {              // ← dep_name is "<name> <version>" or bare name
    let key = (ecosystem.clone(), normalize_dep_name(&ecosystem, dep_name));
    if let Some(to) = name_to_purl.get(&key) { ... }
}
```

The graph builder picks the right version copy AUTOMATICALLY when the reader hands it `"<name> <version>"` because the secondary `nv_key` is registered for every entry in the reader's output. **This is exactly the multi-version-integrity mechanism FR-005 requires**, and it already exists for the npm ecosystem.

**Correction to FR-004**: the plan phase strictly narrows the spec's Q1 clarification. Where the spec says "populate `depends` with target PURLs", read "populate `depends` with target `<name> <version>` strings". The implementation shape is unchanged (still Option A from Q1 — two-pass reader, no emit-time hook); only the disambiguation-string format is corrected. The plan asks the operator to accept a spec amendment before implementation begins (or, if the operator disagrees with the correction, to override the plan back to the PURL shape in tasks.md).

**Alternatives considered**:
- **Populate with raw PURLs directly.** Rejected: diverges from `package_lock.rs` + `pnpm_lock.rs` convention; requires new `name_to_purl.get()` code path in the graph builder to accept PURL keys directly. High risk of missing an emit-time resolver code path and shipping subtly-wrong edges.
- **Populate with parent-qualified key-paths (Option B from Q1).** Rejected during Q1. Same reasons re-apply: introduces a new emit-time resolver hook.
- **Populate with a `Vec<Purl>` newtype instead of `Vec<String>`.** Rejected: requires changing `PackageDbEntry.depends: Vec<String>` shape used by every other reader — out of scope for a single-reader fix.

## R2: Scope-aware key-path resolver algorithm

**Decision**: Implement `resolve_bun_key(parent_key: &str, dep_name: &str, packages_keys: &HashSet<&str>) -> Option<String>` as a pure function that:

1. **Splits `parent_key` into scope-aware segments.** A segment is either `@<scope>/<name>` (two-segment atomic) or `<name>` (one-segment atomic). Slashes INSIDE a scope prefix don't count as segment boundaries.
2. **Walks the segment list from most-specific to root**, at each level constructing a candidate lookup key `<segments[0..=level]>/<dep_name>` and checking it against `packages_keys`.
3. **Falls back to bare `<dep_name>`** if no prefix walk hits (root-hoisted lookup).
4. **Returns `Some(matched_key)` on success**, `None` on complete miss.

**Rationale**:
- **Matches bun's node_modules install-chain semantics.** Bun with `--linker=isolated` installs each package at `node_modules/.bun/<name>@<version>/node_modules/<name>` and encodes the install chain into the lockfile key path. Resolution walks that chain from the current package's directory upward, first-match wins — same as node_modules' historical behavior.
- **Scope-aware segmentation is essential.** `"@fast-csv/format"` is ONE segment (spanning two slash-delimited path components); splitting on bare `/` would produce `["", "fast-csv", "format"]` which mis-locates the scope boundary. FR-003 explicitly requires this handling.
- **Pure function → testable in isolation.** No filesystem access, no I/O, no allocations beyond `String` construction for candidate keys. Deterministic; a single input → single output.

**Alternatives considered**:
- **Convert `parent_key` to a nested-tree structure once, then walk the tree.** Rejected: over-engineered for the resolver's O(depth) lookup count. String-slicing the key at each level is `O(1)` per iteration and there's no data structure to maintain.
- **Cache per-`parent_key` walk results in a `RefCell<HashMap>`.** Rejected: pass-2 iterates `packages` once, each parent's dep-name resolutions are unique to that iteration. No repeated lookups to amortize.
- **Fall back to node_modules directory walk if lockfile lookup misses.** Rejected: (a) bun's `--linker=isolated` puts packages under `node_modules/.bun/...` where the walker wouldn't find them; (b) the fix scope explicitly excludes node_modules fallback per the reporter's rationale in issue #723 (top-level `node_modules/` contains only ~2% of the tree on bun-isolated).

**Test vectors** (locked in for SC-005 unit tests):
```
parent_key: "@fast-csv/format/@types/node"
dep_name: "tslib"
candidates walked (in order):
  1. "@fast-csv/format/@types/node/tslib"
  2. "@fast-csv/format/@types/tslib"
  3. "@fast-csv/format/tslib"
  4. "tslib"
```

```
parent_key: "foo/bar/baz"
dep_name: "@scope/pkg"
candidates walked:
  1. "foo/bar/baz/@scope/pkg"
  2. "foo/bar/@scope/pkg"
  3. "foo/@scope/pkg"
  4. "@scope/pkg"
```

```
parent_key: "lodash"  (root-hoisted parent)
dep_name: "chalk"
candidates walked:
  1. "lodash/chalk"
  2. "chalk"
```

## R3: Four dep-section walker + optional/optional-peers tagging

**Decision**: Walk the metadata object's four sub-maps in this order: `dependencies` → `peerDependencies` → `optionalDependencies` → `optionalPeers`. For each `(section, dep_name, range)` triple, resolve `dep_name` via R2's walker. On resolution success:
- Sections `dependencies` and `peerDependencies`: append `"<name> <version>"` to the parent's `depends` set. Regular runtime edge.
- Section `optionalDependencies`: append `"<name> <version>"` AND tag the parent's `PackageDbEntry.lifecycle_scope` field... **no wait**. The lifecycle_scope belongs on the TARGET entry (m180 pattern: the TARGET is the optional dep), not the parent. Correcting: the edge itself carries the optional scope via `RelationshipType::OptionalDependsOn`, and the target's `lifecycle_scope` field records the scope for the m179 emission machinery.

**Rationale**:
- **Section order is m180 verbatim** (`package_lock.rs:207-212`): four sections walked in the same order. Matching order guarantees identical edge-set ordering for equivalent lockfile shapes across bun and npm classic.
- **Target-side tagging semantics come from m179/m180** (`waybill_common/src/resolution.rs:415, 525, 623`). The `LifecycleScope::Optional` enum variant lives on the target's `PackageDbEntry`, and `RelationshipType::OptionalDependsOn` differentiates the edge kind. The emission machinery (CDX `scope: "optional"`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`, SPDX 3 `LifecycleScope::Optional`) is all downstream of this pair.
- **`waybill:optional-derivation` annotation strings** follow the reader-derivation convention documented at `docs/reference/sbom-format-mapping.md` C42: `"npm-optional-dependencies"` (package_lock), `"yarn-optional-dependencies"` (yarn), `"pip-extras"` (pip). Extending to `"bun-optional-dependencies"` + `"bun-optional-peers"` matches the pattern exactly.

**Alternatives considered**:
- **Merge `optionalDependencies` and `optionalPeers` into a single walk.** Rejected: `waybill:optional-derivation` annotation string differs between them; downstream operators use the string to distinguish "optional (may not be installed)" from "optional peer (may not be provided)".
- **Skip `peerDependencies` entirely because bun's isolated linker may not install them.** Rejected: FR-006 explicitly names `peerDependencies` as a required edge source. Unmet peers are handled by R2 returning `None` and FR-011 warn-and-drop.

## R4: `overrides` interaction with edge resolution

**Decision**: `overrides` continue to apply at the **component-emission** step (pre-fix behavior at `bun_lock.rs:235-238`). The edge resolver operates on the OVERRIDDEN version because the `packages_keys` set the resolver consults is built from the ALREADY-emitted components (each carrying its overridden version). No new code path needed — the fix reuses the pre-fix override machinery verbatim.

**Rationale**: `overrides` semantics say "when this name is referenced anywhere, resolve to this version." Pre-fix `bun_lock.rs:235-238` applies the override to the component's version at emission time. The resolver's `packages_keys` HashSet (pass 1 output) is built from the emitted components, so it naturally reflects the override. No edge is emitted to the un-overridden version because no un-overridden component exists.

**Verification**: SC-001 fixture will include an override case if it's convenient to add. If not, a dedicated unit test covers the interaction.

## R5: FR-010 warn-and-continue semantics per edge-case category

**Decision**: Warn-and-continue with distinct reason-strings per category, all namespaced `bun.lock edge dropped`:

| Edge case | Log line |
|-----------|----------|
| Metadata object missing at `packages[K][2]` | `bun.lock edge dropped: parent={K} reason=metadata_absent` |
| Metadata object at `[2]` is not a JSON object | `bun.lock edge dropped: parent={K} reason=metadata_malformed` |
| Dep-name resolves to no packages-map key (via R2 walker) | `bun.lock edge dropped: parent={K} dep={X} reason=unresolved` |
| Dep-name value in metadata is null or empty string | `bun.lock edge dropped: parent={K} dep={X} reason=empty_range` |

**Rationale**:
- **Constitution X (Transparency)** demands consumers can act on data waybill can't guarantee. Reason strings let downstream SBOM audit pipelines quantify "how many edges did waybill drop and why" without reader-source-code archeology.
- **Consistent namespace** (`bun.lock edge dropped:`) enables `grep 'bun.lock edge dropped'` for a scan-wide count. Matches the m147 issue #262 log-line convention.
- **Distinct reason strings per category** support triage. An operator seeing 100 `metadata_malformed` lines knows to check the lockfile-editor tool; seeing 100 `unresolved` lines knows to check the install chain.

**Alternatives considered**:
- **Emit a `PackageDbEntry.extra_annotations` entry recording dropped-edge stats instead of / in addition to `tracing::warn!`.** Rejected as scope creep: this would introduce a NEW `waybill:*` annotation family, requiring a m071 C-row, extractor, and cross-format-parity plumbing. Not blocking; can land in a follow-up spec if operators demand programmatic access to the drop-log.
- **Fail the reader if ≥N edges drop from one lockfile.** Rejected: violates the m106 FR-010 warn-and-continue posture. The reader hands the graph builder what it CAN resolve; downstream `graph completeness = partial` + the m167 orphan-reason classifier already surface the impact.

## R6: `docs/ecosystems.md` update surface

**Decision**: Add a brief clarifying footnote to the npm ecosystem row's "Dep-graph" column stating explicit coverage for `package-lock.json`, `pnpm-lock.yaml`, AND `bun.lock` (post-m667). If the row currently reads "Lockfile (full tree)" without qualifier, the update MAY be a single-line addition below the row confirming coverage; if it reads "Lockfile (full tree — package-lock.json + pnpm-lock.yaml)" with a bun caveat pre-fix, the update REMOVES the caveat.

**Rationale**: Constitution X (Transparency) + FR-012 both point at the docs matrix telling the truth. The exact edit shape depends on the pre-fix `docs/ecosystems.md` npm-row wording, which will be inspected during T-tasks generation.

**Verification**: SC-008 unit is manual doc-review + `grep -i bun docs/ecosystems.md` returns honest coverage.

## Constitution re-check post-research

All Phase 0 decisions preserve every principle:
- **VIII (Completeness)**: The fix RESTORES conformance.
- **XII (External Data Source Enrichment)**: FR-008 (no new components) verified via R1 (edges land only on pre-emitted components). Provenance annotation carried through `PackageDbEntry.source_path` per pre-fix behavior.
- **IX (Accuracy)**: R2's scope-aware resolver + m147's `<name> <version>` disambiguation together guarantee multi-version integrity.
- **X (Transparency)**: R5's warn-log convention gives operators grep-able drop signals.
- **V (Specification Compliance)**: R3's optional-derivation strings + `LifecycleScope::Optional` reuse mean zero new `waybill:*` annotations invented.

**Open item pending operator approval**: R1 corrects the spec's Q1 clarification from "PURLs" to `"<name> <version>"`. This is a factual correction (the spec's Q1 recommendation was based on my misreading of package_lock.rs), not a scope change. Recommend the plan lands the correction inline; if operator prefers a spec amendment via `/speckit.clarify` before proceeding, halt the plan phase here.

Ready to proceed to Phase 1 design.
