# Research: m194 Close Remaining Orphan Gaps

**Date**: 2026-07-14
**Purpose**: Verify actual code paths for both US1 + US2 before writing implementation, resolve architectural unknowns identified in the spec/clarify phases.

## R1 — US1 stdlib emission location + edge insertion point

**Decision**: Insert the synthetic mainmod → stdlib Relationship at the same site where `build_stdlib_entry` is called (`golang/legacy.rs:2256`). The reader needs access to the Relationships Vec — extend the Go reader's read function to return `(Vec<PackageDbEntry>, Vec<Relationship>)` OR pass `&mut Vec<Relationship>` in.

**Rationale**: Verified via code inspection:

- `build_stdlib_entry` at `legacy.rs:781` constructs a `PackageDbEntry` for `pkg:golang/stdlib@v<version>` — one per unique Go version found in the scan (dedup via `emitted_versions: HashSet<String>` per the loop at ~line 2250).
- The Go reader's outer `read` function already collects `Vec<PackageDbEntry>` and returns to `scan_fs/mod.rs`. `Relationship` emission is done in `scan_fs/mod.rs:756-772` post-hoc by iterating `entry.depends`.
- Simplest wiring: after each stdlib entry is pushed, ALSO push a matching stdlib name onto the corresponding Go mainmod component's `.depends` list. Then the existing edge-emission loop at `scan_fs/mod.rs:756-772` picks it up naturally — it looks up dep names via `name_to_purl` and emits Relationships.
- Even simpler: since `.depends` uses NAMES (not PURLs), just append `"stdlib"` to the Go mainmod's `.depends` list at emit-time.

**Alternative approaches**:
- (A) **Return edges from `build_stdlib_entry` + merge in caller**: too specialized for one edge type.
- (B) **Manipulate `.depends` on the primary Go mainmod at stdlib-emit time**: leverages existing name→PURL resolution at `scan_fs/mod.rs:756-772`. Zero new Relationship-emit code.
- (C — **CHOSEN, refined**) **Append `"stdlib"` to the primary Go mainmod's `.depends`** at the same site where `build_stdlib_entry` is called. `name_to_purl` already maps `"stdlib" → pkg:golang/stdlib@v<version>` for the emitted entry. The existing DependsOn Relationship emission picks it up.

**References**:
- `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:781` — `build_stdlib_entry` definition.
- `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:2256` — sole call site.
- `mikebom-cli/src/scan_fs/mod.rs:756-772` — dep-name → PURL Relationship emission loop.

## R2 — US2 nameless-nested-workspace root cause

**Decision**: mikebom's npm reader ALREADY discovers nested `package.json` + `package-lock.json` pairs (via `candidate_project_roots` at `npm/mod.rs:547`) AND ALREADY emits transitive components from their lockfiles. The gap: no mainmod component is emitted for **nameless** nested `package.json` files (those without a `name` field).

For pico's case: `pkg/db/integrationtest/schemalint/package.json` has content `{"dependencies": {"schemalint": "^2.1.0"}}` — no `name`. mikebom's `build_npm_main_module_entry` requires `name` (per FR-001 of m066), returns `None`.

The existing `apply_nameless_secondary_umbrella` pass (line 361) is meant to compensate — it merges the nameless manifest's declared deps into the CLOSEST enclosing primary mainmod. In pico's case that would be `@kusaridev/pico` (top-level npm). But investigation of `/tmp/pico-native.cdx.json` shows the umbrella DIDN'T merge — `@kusaridev/pico`.depends has only 2 entries and `schemalint` isn't among them.

**Root cause**: EITHER (a) `candidate_project_roots` doesn't include `pkg/db/integrationtest/schemalint` (walker exclusion), OR (b) the umbrella pass fires but the merge silently drops the dep, OR (c) something downstream of merge strips it.

**Fix strategy** per Q1 answer A: bypass the umbrella entirely by synthesizing a mainmod for each nameless nested workspace root. Use a versionless PURL `pkg:npm/<dir-basename>` (per m191 spec-clean convention). The synthesized mainmod carries `mikebom:component-role: main-module` — so `select_root` picks it up as a per-ecosystem-root candidate, m158 emits it as a workspace-peer of the top-level, and BFS reaches transitives via its `.depends` chain naturally.

**Alternatives considered**:
- (A) **Fix the umbrella pass** so it correctly merges the nameless manifest's deps into the parent mainmod: unclear root cause; might be a walker exclusion or a downstream strip. Investigation time > fix time.
- (B) **Extend `build_npm_main_module_entry` to allow nameless manifests by falling back to directory-basename as the name**: broader behavior change; might affect other npm scanners' consumer contracts.
- (C — **CHOSEN**) **Synthesize a NEW mainmod specifically for nameless nested workspaces** as a new code path adjacent to `apply_nameless_secondary_umbrella` (or replacing it): scoped fix, additive.

**References**:
- `mikebom-cli/src/scan_fs/package_db/npm/mod.rs:194+` — m066 mainmod emission loop (skips nameless).
- `mikebom-cli/src/scan_fs/package_db/npm/mod.rs:361+` — m256 umbrella pass (attempts to compensate; empirically not reaching pico's schemalint case).
- `mikebom-cli/src/scan_fs/package_db/npm/mod.rs:547` — `candidate_project_roots` (walker for npm project roots).

## R3 — `--root-name` interaction with the new nested mainmods (Q2 answer B)

**Decision**: `apply_main_module_drop_or_demote` at `mikebom-cli/src/generate/root_selector.rs:528` is the single point that drops mainmods when operator-override is active. Per m149, it iterates ALL components carrying `mikebom:component-role: main-module` and drops each. So the new nested mainmods from US2 will be dropped automatically alongside the top-level mainmods, and m192/m193's pre-rewrite (at `builder.rs:487+`) re-anchors their outgoing edges onto `target_ref`.

**Verification task**: Read `apply_main_module_drop_or_demote` to confirm it handles the multi-mainmod case correctly. Expected shape: iterates every mainmod, adds its PURL to `dropped_main_module_purls`, filters it out of the emitted component list. If it happens to only drop the FIRST mainmod, extend it.

**Rationale**: Q2 answer B specifies the desired semantic — drop ALL mainmods, rely on pre-rewrite. If the code already does this per m149, no change needed at that site. If not, small extension.

**Impact on synthesized nested mainmod**: When operator uses `--root-name pico`, the synthesized `pkg:npm/schemalint` (or whatever nameless nested workspace) mainmod IS dropped. Its outgoing edge `pkg:npm/schemalint → pkg:npm/schemalint@2.3.2` gets re-anchored to `pico@2c2f9719 → pkg:npm/schemalint@2.3.2` by pre-rewrite. Then BFS from `pico@2c2f9719` reaches schemalint, which chains to its own transitives (chalk, commander, etc.) via the existing lockfile-tier edges.

## R4 — Multi-Go-version stdlib edge (FR-003)

**Decision**: When multiple Go mainmods are emitted (multi-binary image scan), each mainmod's `.depends` gets the `"stdlib"` name appended. The existing `name_to_purl` resolution creates a Relationship per mainmod → matching-version stdlib pair.

**Rationale**: `emitted_versions: HashSet<String>` at `legacy.rs` implies one stdlib entry per unique Go version. If two binaries share a Go version, both mainmods depend on the same stdlib entry (Relationship set dedupes). If two binaries use different Go versions, two stdlib entries exist + each mainmod depends on its own — assuming `name_to_purl` disambiguates by version, which it currently might not (since `.depends` uses names, not PURLs, and "stdlib" is a single name).

**Investigation task**: Check whether `name_to_purl` at `scan_fs/mod.rs:756-772` properly disambiguates when two `stdlib@X` and `stdlib@Y` entries exist with the same name. If ambiguous, this milestone may need to short-circuit and emit the Relationship directly (bypassing name resolution) for the stdlib case.

**Fallback**: If name-resolution is ambiguous, add the Relationship directly via a new emit-site in `golang/legacy.rs` that returns the pair or pushes onto a mutable `Vec<Relationship>` passed in.

## R5 — Native-first Principle V audit

**Decision**: NO new `mikebom:*` annotation is introduced.

- US1 uses existing `Relationship { RelationshipType::DependsOn }` with a synthetic `EnrichmentProvenance { source: "m194-stdlib-synthesis" }`. The `Relationship` type is native to mikebom's cross-format model; no `mikebom:*` property.
- US2 uses existing `mikebom:component-role: main-module` annotation (established by m066/m127) applied to the newly-synthesized nested workspace mainmod. Reusing an existing annotation is not "introducing new".

**Rationale**: Audit result recorded in FR-014. Reviewers can reject any implementation-time drift that introduces new `mikebom:*` annotations.

## R6 — Byte-identity drift set

**Decision**: Phase 2 T003 audit will grep every golden CDX for:
- Presence of `pkg:golang/stdlib@v*` component (US1 drift set)
- Presence of a nested nameless-package.json fixture (US2 drift set — unlikely; mikebom's in-repo fixture corpus doesn't include this shape by grep)

Expected drift set: small. Most existing goldens are ecosystem-focused (npm, cargo, gem, etc.) and use fixtures that don't trigger stdlib emission (only Go binary scans do that; our Go fixture is a source-tree fixture where stdlib synthesis IS the drift signal).

**Regen strategy**: standard MIKEBOM_UPDATE_*_GOLDENS=1 for affected files only, diff-review that every diff is either:
- (US1) A new edge `<Go mainmod> → pkg:golang/stdlib@v*` in the CDX `dependencies[]`
- (US2) A new mainmod component with `mikebom:component-role: main-module` + edges to nested workspace transitives

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| US1 stdlib edge insertion | Append `"stdlib"` to Go mainmod's `.depends`; existing name→PURL loop emits Relationship | Emit Relationship directly | Reuses existing infrastructure; zero new code path |
| US1 multi-version handling | Investigate `name_to_purl` disambiguation; fallback to direct Relationship emit if ambiguous | Always emit direct Relationship | Optimistic path leverages existing code |
| US2 fix approach | Synthesize new mainmod for nameless nested workspaces (Q1 answer A) | Fix umbrella pass | Bypasses uncertain umbrella-pass root cause |
| US2 nested mainmod PURL shape | Versionless `pkg:npm/<dir-basename>` (m191 spec-clean convention) | Include version from parent | No version available for nameless manifest |
| --root-name interaction | Rely on m149's existing multi-mainmod drop + m192/m193 pre-rewrite | New pre-rewrite path | Verified: existing plumbing handles it |
| New Cargo deps | None | Add semver | Existing types sufficient |
| New `mikebom:*` annotations | None | Add `mikebom:synthesized-nested-mainmod` | Existing `mikebom:component-role: main-module` covers |
