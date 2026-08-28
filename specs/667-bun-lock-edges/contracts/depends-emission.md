# Contract: bun.lock → PackageDbEntry.depends emission

## Interface

### Input

- **File**: `<rootfs>/bun.lock` (existing input; the fix does not change what triggers `read_bun_lock`).
- **Shape**: JSONC serialized `serde_json::Value` with the m106 schema (`workspaces` map, `packages` map with 4-tuple values, `overrides` map).
- **Position 2 of each `packages` value**: the metadata object — pre-fix unread, post-fix the sole edge source (see FR-001).

### Output

- **`Vec<PackageDbEntry>`**: return of `parse_bun_lock`. Component set IS UNCHANGED vs pre-fix (FR-008, FR-009). What changes:
  - **Non-workspace entries**: `depends: Vec<String>` populated with `<name> <version>` disambiguation strings per R1.
  - **Targets referenced exclusively via optional/optional-peers sections**: `lifecycle_scope: Some(LifecycleScope::Optional)` set, `extra_annotations["waybill:optional-derivation"] = "bun-optional-dependencies"` or `"bun-optional-peers"` per R3 + data-model V6.

## Behavioral contract

### C1: Edge-source completeness

For every `packages[K]` entry with a well-formed metadata object at position 2, the reader MUST attempt edge extraction from ALL FOUR sub-map sections in this order: `dependencies` → `peerDependencies` → `optionalDependencies` → `optionalPeers`. Missing sections = zero edges from that section; malformed sections = warn-and-drop per C5.

### C2: Scope-aware resolver correctness

The R2 `resolve_bun_key` function MUST produce the exact test-vector outputs from research.md R2 for the three test-vector inputs (nested-scope, cross-scope target, hoisted-parent). Verified via unit test in `bun_lock.rs`'s test module.

### C3: Disambiguation-string format

For each resolved edge, the reader MUST append `format!("{} {}", target_name, target_version)` to the parent's `depends` set — matching `package_lock.rs:261` verbatim. **Note this deliberately differs from the spec's Q1 clarification**, which said "PURLs directly." See research R1 for why the codebase convention is name-version strings.

### C4: Multi-version integrity

Given N different `packages`-map keys sharing the same package name at N different versions (a state bun's non-hoisted linker produces), the reader MUST emit N distinct entries in the parent's `depends` list — one per version — so the graph builder's secondary `name_to_purl` key `(ecosystem, "<name> <version>")` resolves each to the correct version copy per R1. Verified via SC-004 fixture + unit test.

### C5: FR-011 warn-and-drop

Every dropped edge MUST emit exactly ONE `tracing::warn!` line with the R5 format `bun.lock edge dropped: parent={K} [dep={X}] reason={reason}`. Reason strings drawn from R5's table verbatim: `metadata_absent`, `metadata_malformed`, `unresolved`, `empty_range`. Verified via unit tests that scan a `tracing_test`-captured log buffer.

### C6: FR-008 component-set invariant

`components.len()` pre-Pass-2 MUST equal `components.len()` post-Pass-2. Enforced by unit test `test_pass2_preserves_component_count`.

### C7: Optional-scope precedence

A target reached via BOTH a hard section AND an optional section MUST NOT be tagged optional (target's `lifecycle_scope` stays `None`). A target reached exclusively via `optionalDependencies` MUST be tagged `waybill:optional-derivation = "bun-optional-dependencies"`. A target reached exclusively via `optionalPeers` MUST be tagged `waybill:optional-derivation = "bun-optional-peers"`. A target reached via BOTH `optionalDependencies` AND `optionalPeers` (no hard section) MUST be tagged `"bun-optional-dependencies"` (the `-dependencies` variant wins, matching m180 precedence).

### C8: Workspace-path preservation

The workspace-member emission loop at `bun_lock.rs:105-197` MUST run BEFORE Pass 1 and BEFORE Pass 2 (order preserved from pre-fix state). Workspace entries' `depends` populate ONLY from `workspace:*` values (pre-fix behavior); the new Pass 2 walker MUST NOT touch workspace entries — they're identified by their `waybill:component-role = "main-module"` annotation (pre-fix `bun_lock.rs:161-163`) OR by presence in `workspace_member_names` (pre-fix line 116).

### C9: Override interaction

The pre-fix override machinery at `bun_lock.rs:235-238` remains authoritative. The `PackagesKeysIndex` built in Pass 1 uses the OVERRIDDEN version for the disambiguation string, so resolved edges point at the overridden component automatically (no separate override-in-edge-resolver step).

## Non-contracts

- **The reader does NOT emit new components based on lockfile-only presence.** Every edge target MUST resolve to a `packages`-map key whose component the pre-fix reader ALREADY emits. Unresolved dep-names get FR-011 warn-and-drop, never phantom-component creation. Matches Constitution XII.
- **The reader does NOT touch the `overrides` map's own parsing** — pre-fix code at `bun_lock.rs:89-97` is unchanged.
- **The reader does NOT emit hash annotations from position 3** of the tuple (deferred to a separate feature; scoped out per spec Key Entities).
- **The reader does NOT fall back to walking `node_modules/`** on any lookup failure. Bun's isolated linker layout makes this hostile (~2% coverage per issue #723 reporter's investigation); the lockfile is the reliable edge source.

## Test-authoring rules

### T1: Reader unit test locations

New unit tests land inside `bun_lock.rs`'s `#[cfg(test)] mod tests` block per m106 precedent. Additive-only — no pre-fix test may be modified or deleted (FR-007).

### T2: Fixture layout

Each SC-001/SC-003/SC-004/SC-005/SC-007 fixture lands at `waybill-cli/tests/fixtures/bun_lock/<slug>/{package.json, bun.lock}`. Fixtures are lockfile-only (no `node_modules/`) to avoid tempting `#[cfg(test)]` code into cross-checking against an installed tree.

### T3: Integration test structure

`waybill-cli/tests/bun_lock_edges_us1.rs`: runs `waybill sbom scan --path <fixture> --offline --format cyclonedx-json --output <tempfile>` per fixture, parses emitted CDX, asserts:
- `dependencies[]` contains the expected edges (per fixture's US1 acceptance scenario).
- `waybill:orphan-reason` annotation absent from expected-reachable components.
- Optional-fixture only: `scope: "optional"` on the expected optional edges.

Follows the m665 `no_binary_scan_us3_annotation.rs` integration-test shape (env_guard, tempfile, subprocess spawn via `env!("CARGO_BIN_EXE_waybill")`).

## Spec amendment recommendation

Per research R1, the spec's FR-004 and Q1 clarification say "populate `depends` with PURLs directly." The actual convention is `<name> <version>` strings. The plan phase RECOMMENDS the following spec edit before implementation begins:

- **FR-004 replace**: "populate `PackageDbEntry.depends: Vec<String>` on the PARENT entry with the target's *PURL* string" → "populate `PackageDbEntry.depends: Vec<String>` on the PARENT entry with a `\"<name> <version>\"` disambiguation string matching the `package_lock.rs:261` convention; the graph builder resolves to a target PURL via the existing secondary `name_to_purl` key at `scan_fs/mod.rs:635-644`."
- **Clarifications 2026-08-27 Q1 answer replace**: "populate with target PURLs directly at reader time" → "populate `depends` with `\"<name> <version>\"` disambiguation strings at reader time via a two-pass approach; graph builder resolves the strings to PURLs at edge-emission time via the existing name_to_purl secondary key."

If the operator prefers to keep the spec's PURL claim and diverge from the m147 convention, the plan can adapt — but the tasks.md phase would need to include additional graph-builder code to accept raw PURL keys in `name_to_purl`. Recommend the operator accept the correction inline.
