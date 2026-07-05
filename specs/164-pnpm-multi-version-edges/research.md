# Research: milestone 164 — pnpm v9 multi-version edge disambiguation

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Phase 0 research. Milestone 164 was empirically-grounded before spec time (root cause verified 2026-07-05 on live podman-desktop), so this research documents the design decisions rather than exploring unknowns.

## R1 — Exact line-number pinpoint of the bug

**Decision**: The bug lives at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:80-84`.

```rust
let Some((canon_name, _canon_ver)) = parse_pnpm_key(stripped) else {
    tracing::debug!(dep = %dep_pair_raw, "pnpm-lock: skipping non-registry dep value");
    continue;
};
deps.push(canon_name);   // ← BARE NAME — version discarded via `_canon_ver`
```

`parse_pnpm_key` correctly extracts both `(canon_name, canon_ver)` from peer-dep-suffixed values like `"@algolia/autocomplete-core@1.17.9(algoliasearch@5.42.0)(search-insights@2.17.3)"` — the version `"1.17.9"` is available in-scope but immediately discarded with the `_canon_ver` underscore. The bare `canon_name` propagates into `snapshots_lookup` values, then into per-package `PackageDbEntry.depends`, then into edge-resolution at `scan_fs/mod.rs:729-731` where the name-only key hits last-write-wins between multiple emitted versions.

**Rationale**: The version discard is the load-bearing single line. Everything else is in place: `parse_pnpm_key` extracts it, `scan_fs/mod.rs:519-525` indexes emitted entries by both bare-name AND `"<name> <version>"` disambiguation-key form, and the edge-resolution loop already tolerates both key shapes. Only the parser's emit-side needs to change.

**Alternatives considered**:
- **A. Change the emit-side lookup at `scan_fs/mod.rs:729-731`**: rejected — the resolver code is ecosystem-neutral and shared with cargo/maven. Adding pnpm-specific logic there would violate the ecosystem-neutral abstraction.
- **B. Add a post-parse disambiguation pass**: rejected — the parser has the version info in-scope; discarding it and re-computing later is silly.
- **C. Fix at the discard site (chosen)**: minimal diff, no cross-cutting changes.

## R2 — Threading the version through `collect_pnpm_dep_names`

**Decision**: Add a boolean parameter `emit_versioned: bool` to `collect_pnpm_dep_names`. When `true`, push `format!("{canon_name} {canon_ver}")` (disambiguation form). When `false`, push bare `canon_name` (pre-164 behavior).

**Function signature change**:
```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,    // ← NEW
) -> Vec<String>
```

**Call sites**:
1. `build_snapshots_lookup` (line 122) — pass `emit_versioned = true` (v9 snapshot values carry peer-dep-suffixed keys with versions; disambiguation form is safe).
2. v6/v7 inline path (line 262) — pass `emit_versioned = false` (User Story 2 byte-identity guard; v6/v7 fixtures don't need the disambiguation form because the existing bare-name path already works for the pre-v9 lockfile shape).

**Rationale**: Parameter is more explicit than an implicit callsite-inspection pattern. `bool` is minimal state — no enum needed because the two states are truly binary. Both callers are static/deterministic (no runtime branching on lockfile content beyond version detection which already exists).

**Alternatives considered**:
- **A. Add `emit_versioned: bool` parameter (chosen)**: minimal API change, explicit at every call site, easy to unit-test.
- **B. Duplicate `collect_pnpm_dep_names` into `collect_pnpm_dep_names_versioned`**: rejected — introduces divergence risk (the two functions could drift). Parameterization keeps one code path.
- **C. Always emit versioned form**: rejected — violates User Story 2 (v6/v7 byte-identity guard). Even though `depends` isn't directly serialized, changing it changes intermediate state that some tests inspect.
- **D. Global mutable flag or thread-local**: rejected — invisible dependency, harder to test.

## R3 — Malformed peer-dep-suffixed key handling (FR-008)

**Decision**: If `parse_pnpm_key` returns `None` (malformed key), the existing `continue` at line 82 already handles this — the whole dep is dropped from `deps` and a debug-level log fires (line 81). This is the pre-164 behavior. Milestone 164 does NOT change error handling.

However, FR-008 mandates that when the version is empty/malformed (e.g., `parse_pnpm_key` returns `("foo", "")` — unlikely but possible if the key form drifts), the parser MUST fall back to bare-name emission WITH a `tracing::warn!` (not just debug). We add:

```rust
if emit_versioned {
    if canon_ver.is_empty() {
        tracing::warn!(
            key = %stripped,
            "pnpm-lock v9: peer-dep-suffixed key parsed to empty version; falling back to bare-name form"
        );
        deps.push(canon_name);
    } else {
        deps.push(format!("{canon_name} {canon_ver}"));
    }
} else {
    deps.push(canon_name);
}
```

**Rationale**: Constitution Principle VIII (Completeness) — never silently drop. Constitution Principle X (Transparency) — the warn log makes the fallback observable. If the malformed-key case fires in the wild, CI logs surface it.

**Alternatives considered**:
- **A. Fall back silently to bare-name**: rejected — violates Principle X (Transparency).
- **B. Fail hard (return error)**: rejected — violates Principle VIII (Completeness); a single malformed dep shouldn't fail the whole scan.
- **C. Warn + fall back to bare-name (chosen)**: matches milestone-055/091/160 pattern for degraded-mode recovery.

## R4 — FR-009 tracing log fields

**Decision**: The existing `pnpm-lock parsed` info-level log at `pnpm_lock.rs:373-377` already carries `packages_count`, `snapshots_count`, `fell_back_to_snapshots`. Milestone 164 EXTENDS this line with two new fields:
- `multi_version_disambiguated_count` — count of snapshot deps where `emit_versioned=true` produced a versioned form (i.e., every non-empty-version snapshot dep emitted through the new path).
- `malformed_key_warn_count` — count of `tracing::warn!` fallbacks per R3.

These are cheap accumulators (two `usize` locals). The log line becomes:
```
pnpm-lock parsed lockfile=<path> lockfile_version=9.0 packages_count=N snapshots_count=M fell_back_to_snapshots=M multi_version_disambiguated_count=K malformed_key_warn_count=W
```

**Rationale**: Extending the existing log line preserves the milestone-157/158/159/160/161/162/163 observability convention (single grep-friendly summary per lockfile). Adding two fields is trivially backward-compat for consumers doing regex parsing.

**Alternatives considered**:
- **A. Extend existing log (chosen)**: minimal footprint, preserves convention.
- **B. Emit a separate `milestone-164 summary` log**: rejected — noisy, doesn't compose with the existing per-lockfile log.

## R5 — Test strategy (SC-007 + SC-008 + SC-010)

**Decision**: Three test tiers, mirroring milestone-160/161/162/163 pattern.

**Tier 1: Unit tests** (SC-007, ≥8 tests inside `pnpm_lock.rs`'s existing `#[cfg(test)] mod tests` block):
- **T007** `collect_pnpm_dep_names_emit_versioned_true_produces_versioned`: minimal snapshot mapping with peer-dep-suffixed value; assert `deps` contains `"foo 1.2.3"`.
- **T008** `collect_pnpm_dep_names_emit_versioned_false_preserves_bare_name`: same input, `emit_versioned=false`; assert `deps` contains `"foo"` (pre-164 behavior).
- **T009** `collect_pnpm_dep_names_empty_version_falls_back_with_warn`: synthesize a malformed key where `parse_pnpm_key` returns empty version; assert `deps` contains bare `"foo"` (fallback). WARN log verification via `tracing_subscriber::fmt::TestWriter` or capture stub.
- **T010** `collect_pnpm_dep_names_v6_v7_bare_name_unchanged`: pnpm v6/v7 inline path uses `emit_versioned=false`; assert byte-identical to pre-164 output.
- **T011** `build_snapshots_lookup_emits_versioned_for_v9`: end-to-end via `build_snapshots_lookup`; assert the lookup values contain versioned forms.
- **T012** `parse_pnpm_lock_multi_version_edges_resolve_correctly`: minimal fixture with 2 versions of same package + 2 parents (each declaring a different version); assert each parent's `depends` array carries the correct `"<name> <version>"` form.
- **T013** `parse_pnpm_lock_purl_never_includes_peer_dep_suffix` (FR-005): assert emitted PURLs never contain `(`.
- **T014** `peer_dependencies_handling_unchanged_after_164` (FR-010): assert the pnpm parser's `peerDependencies:` handling is unchanged.

**Tier 2: Integration test** (SC-008, `tests/pnpm_multi_version.rs`):
- **T015** `t015_synthesized_multi_version_zero_orphans`: tempdir-based fixture with two workspace packages, each declaring a different version of `@shared/lib`. Invoke the release binary via `env!("CARGO_BIN_EXE_mikebom")`. Assert (a) both `@shared/lib@1.0.0` and `@shared/lib@2.0.0` emitted; (b) parent A's `dependsOn` targets `1.0.0`; (c) parent B's `dependsOn` targets `2.0.0`; (d) both versions BFS-reachable; (e) zero multi-version orphans.

**Tier 3: Optional real-testbed audit** (SC-010, `tests/pnpm_multi_version_audit.rs`):
- **T017** `t017_podman_desktop_multi_version_gap_below_30`: gated behind `MIKEBOM_PNPM_MULTIVER_AUDIT=1` + `MIKEBOM_FIXTURES_DIR` (or a cached podman-desktop clone). If not gated, skip silently. Assert multi-version orphans ≤ 30 AND BFS reachability ≥ 93%.

**Rationale**: Matches milestone-160 T033 + milestone-161 T040 + milestone-162 T034 + milestone-163 T037 fixture-gated audit precedent. Real-testbed audits are OPPORTUNISTIC per the pattern — not blocking for the PR.

## R6 — Empirical baseline (2026-07-05, live podman-desktop)

**Decision**: Pin the empirical baseline to the measurement performed 2026-07-05 with mikebom `bfd0f6d` on a fresh clone of `github.com/podman-desktop/podman-desktop`.

**Baseline numbers**:
- Total components: **2748** (2694 npm + 54 file-tier)
- Reachable via BFS: **2126** = **77.4%**
- Orphans: **568**
- Multi-version orphans: **435** (76.6% of orphans)
- Truly-isolated orphans: **133** (broken down into ~50-100 platform-optional bindings + ~30-50 other; both out of scope for m164)

**Target numbers** (post-164):
- Multi-version orphans: **≤30** (94% reduction)
- BFS reachability: **≥93%** (+15pp)
- Zero SC-002 regression (phantom edges = 0)
- Zero SC-004 regression (empty-version PURLs = 0)

**Rationale**: The multi-version-orphan class accounts for exactly the 15pp gap milestone-164 targets. Achieving ≤30 remaining multi-version orphans (from 435) leaves a small buffer for edge cases (e.g., aliased multi-version composition, malformed lockfile entries) without overpromising.

**Empirical-revision escape hatch**: if implementation reveals additional edge cases that cap improvement below 93%, SC-002 target may be revised inline per the milestone-156/157/158/159/160/161/162/163 empirical-revision pattern.

## R7 — Interaction with milestone-159 alias handling

**Decision**: Milestone-159's alias handling (line 105-125 of pnpm_lock.rs, the `alias_map` + `reverse_map` post-processing) runs AFTER the parse loop. By the time alias rewriting fires, `depends` arrays already carry the disambiguation form. `rewrite_dep_names` (milestone 159's utility) MUST be updated to handle both bare names AND `"<name> <version>"` forms — check the local-name of the input, match against `alias_map`, and if found, replace the name portion while preserving the version portion.

**Concrete example**:
- Pre-164 alias rewrite: `"my-local"` → `"@real/pkg"` (bare name substitution).
- Post-164 alias rewrite: `"my-local 1.0.0"` → `"@real/pkg 1.0.0"` (name substituted, version preserved).

**Rationale**: Alias handling and version disambiguation are orthogonal transformations. Composition = "substitute the name, preserve the version". This preserves the FR-003 composability guarantee (alias syntax at any version).

**Implementation impact**: `rewrite_dep_names` in `alias_mapping.rs` needs a small update. The rest of milestone-159's alias flow is unchanged.

## R8 — SC-003 dual-side byte-identity verification

**Decision**: Non-pnpm-v9 goldens verified via existing test suite (`cargo test --workspace`). Every non-pnpm-v9 milestone-090 golden fixture runs through its existing integration test; if the golden diffed, the test fails and blocks the PR.

**Coverage**:
- 10 non-pnpm ecosystems (apk, bazel, cargo, cmake, deb, gem, golang, maven, pip, rpm) × 3 formats = 30 goldens.
- 1 `npm` fixture (uses `package-lock.json`, not `pnpm-lock.yaml v9`) × 3 formats = 3 goldens.
- Total: **33 goldens must remain byte-identical**.

Pnpm-lock v9 fixtures (if any exist in milestone-090) MAY change. Verified at authoring time: `find mikebom-cli/tests/fixtures/pnpm* -name 'pnpm-lock.yaml' -exec head -3 {} \;` reveals whether they're v6/v7 or v9. If v9 AND multi-version, goldens legitimately drift — regenerate via `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test`.

**Rationale**: Matches milestone-160/161/162/163 SC-003 verification pattern. Zero-diff on non-pnpm-v9 is enforced by the existing golden test infrastructure; no new test code needed for SC-003.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts + quickstart).
