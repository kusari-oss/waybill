# Research — milestone 148 (source-files cross-emitter divergence — union evidence across same-PURL entries)

Phase 0 output. Resolves four implementation-affecting design questions before Phase 1.

## §A — Order-of-operations with existing post-dedup passes (no conflict)

**Decision**: The union pass can land immediately after `deduplicate()` at `scan_fs/mod.rs:750`. No existing post-dedup pass touches `evidence.source_file_paths`; ordering is not load-bearing.

**Verification**: read `mikebom-cli/src/scan_fs/mod.rs:750-773`:

| Line | Pass | Reads `source_file_paths`? | Writes `source_file_paths`? |
|---|---|---|---|
| 750 | `deduplicate(components)` | yes (within-group merge) | yes (within-group merge) |
| 754-756 | `synthesize_cpes(c)` loop | no | no — writes `c.cpes` |
| 763-764 | `maybe_suppress_scan_target_coord` | no | no — returns new `scan_target_coord` value |
| 773 | `tag_main_modules_with_workspace_root` | no | no — writes `c.extra_annotations["mikebom:is-workspace-root"]` |

**Implication**: the union pass landing at lines 751-752 (immediately after `deduplicate()`) is structurally clean. CPE synthesis at lines 754-756 reads `c` mutably but writes only `c.cpes`, which is independent of the canonicalized `source_file_paths` Vec. No reordering risk.

**Alternatives considered**:
- **Land the union pass INSIDE `deduplicate()`** as a second cross-group phase — rejected: muddles the deduplicator's per-group merge semantic with a global cross-group pass. Two distinct algorithms, one function ⇒ harder to test in isolation.
- **Land the union pass at the end of the scan_fs pipeline (after line 773)** — neutral choice. Slightly later than necessary but no functional difference. Preferring the earlier site (line 751) for proximity to the dedup it complements.

## §B — Code-location decision: in-deduplicator-module vs sibling source_files_union.rs

**Decision**: place the union pass as a new `pub fn canonicalize_source_files_by_purl()` in `mikebom-cli/src/resolve/deduplicator.rs`. NOT a sibling module.

**Rationale**:
- The pass is conceptually a **second phase of deduplication** — same domain as the existing `deduplicate()` function, just with a different group-key (full canonical PURL vs the deduplicator's `(ecosystem, name, version, parent_purl)` 4-tuple).
- Co-locating the two passes makes the relationship explicit: a reader landing on the file sees the within-group merge AND the cross-PURL union together, with both doc-comments visible.
- The existing test module at `mikebom-cli/src/resolve/deduplicator.rs#mod tests` provides the test infrastructure (helper component-construction functions, the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention) without duplication.
- A sibling module would force `pub(super)` exports on the helper helpers (or `pub(crate)` exposure), inflating the module surface area for no functional benefit.

**Alternatives considered**:
- **Sibling module `source_files_union.rs`** — rejected: see above; conceptually duplicates the deduplicator's domain.
- **Inline at `scan_fs/mod.rs:751` as a closure or local block** — rejected: 15+ LOC is over the inline threshold; the helper functions need to be unit-testable in isolation (SC-004 + SC-005).

## §C — Idempotence strategy

**Decision**: idempotence is automatic via the choice of `BTreeSet<String>` as the union-collection type. The set-union of any Vec with itself returns the original set; converting back to `Vec<String>` via `.into_iter().collect()` yields the same sorted Vec byte-for-byte.

**Verification** (illustrative; not normative):

```rust
let original: BTreeSet<String> = ["a", "b", "c"].into_iter().map(String::from).collect();
let vec1: Vec<String> = original.iter().cloned().collect();

// Second pass: rebuild from vec1
let second: BTreeSet<String> = vec1.iter().cloned().collect();
let vec2: Vec<String> = second.iter().cloned().collect();

assert_eq!(vec1, vec2);  // byte-identical
```

**Test (SC-005)**: in `deduplicator.rs#mod tests`, construct two `ResolvedComponent` instances sharing a PURL with different paths. Run the canonicalize pass twice. Assert byte-equality of the post-second-pass output against the post-first-pass output.

**Alternatives considered**:
- **HashSet + sort** — rejected: same logical result but the sort step is now a manual operation, and the test assertions need to call `.sort()` to compare. BTreeSet collapses both concerns into the type.
- **Vec + manual sort + dedup_consecutive** — rejected: requires two passes (sort, then dedup_consecutive), more LOC, harder to read.

## §D — Cross-ecosystem coverage audit

**Decision**: the union pass operates on `c.purl.as_str()` keying and is ecosystem-agnostic by construction. No ecosystem-specific code paths needed.

**Audit of `parent_purl`-setting readers** (to identify all ecosystems where the same-PURL multi-entry shape can arise):

```bash
$ grep -rn "parent_purl: Some\|parent_purl:.*Some" mikebom-cli/src/scan_fs/package_db/ | grep -v tests
```

Returns (verified at plan-phase via the read of `maven.rs:3429-3457` + grep):
- **Maven** (`maven.rs:1795`, `:3432-3436`): nested-coord case — `parent_purl = Some(enclosing_purl)` when the coord is vendored inside a fat-jar.
- **Cargo** (`cargo.rs` ~similar): workspace member case — `parent_purl = Some(workspace-root)` when crate is a workspace member.
- **Go** (`golang.rs` ~similar): module vendored under `vendor/` case — `parent_purl = Some(main-module-purl)`.

**For each ecosystem**: when the same PURL appears BOTH as a top-level entry (parent_purl = None) AND as a nested-under-parent entry, the deduplicator's `(ecosystem, name, version, parent_purl)` group-key keeps them as separate groups → same-PURL multi-entry shape → the union pass canonicalizes their `source_file_paths` Vecs.

**Implication**: US2 (cross-ecosystem coverage) gets satisfied for free. The Maven case is the only one explicitly tested in this milestone (SC-003 synthetic fixture); future ecosystem-specific tests can be added without re-touching the union pass logic.

**Out-of-audit-scope**: the user's harness focused on the polyglot-builder-image corpus where Maven is the dominant ecosystem. Cross-ecosystem follow-ups (Cargo workspace vendoring, Go vendor) are out of scope for SC-001 (which is Maven-specific); they may produce additional fix-receipt counts on other audit corpora but those are operator-cadence verification.

## §E — Pre-existing within-group merge behavior

**Decision**: leave the deduplicator's within-group merge at lines 74-78 unchanged. The cross-PURL union OVERWRITES the within-group merge result for any same-PURL multi-entry case; for single-entry cases the within-group merge is the only operation and it stays as-is.

**Verification** of the within-group merge:

```rust
// From deduplicator.rs:74-78
for file_path in other.evidence.source_file_paths {
    if !best.evidence.source_file_paths.contains(&file_path) {
        best.evidence.source_file_paths.push(file_path);
    }
}
```

The within-group merge collects paths in insertion-order (not sorted). For same-`(ecosystem, name, version, parent_purl)`-key components (e.g., the same package detected by two different readers at the same path-context), this produces an insertion-ordered Vec of unique paths.

**Why no harmonization needed**:
- The within-group merge happens WITHIN a deduplicator group. The cross-PURL union happens ACROSS deduplicator groups that happen to share a PURL.
- The cross-PURL union's BTreeSet collapses BOTH layers' contributions into one alphabetically-sorted Vec. The within-group insertion-order is irrelevant once the cross-PURL union writes back.
- For single-entry-per-PURL components (the common case — within-group merge has nothing to merge, OR merges items into a single group), the cross-PURL union is identity. The within-group merge result reaches emit unchanged.

**Net effect**: post-148, the emitted `evidence.source_file_paths` Vec on every component is either:
- (a) The within-group merge result (insertion-ordered) — for single-entry-per-PURL components where the cross-PURL union is identity.
- (b) The cross-PURL union (alphabetically-sorted) — for same-PURL multi-entry components where the cross-PURL union writes back over the within-group merge.

This is acceptable because path-order in `mikebom:source-files` has no documented semantic — consumers parse it as a set. The shift from insertion-order to alphabetical-order on a subset of components is non-breaking (and arguably improves auditability — alphabetical sort is more predictable for diffing).

**Test for the cross-pass invariant (SC-004)**: a unit test asserting that, for a single-entry PURL, the post-148 `source_file_paths` is byte-identical to the pre-148 within-group merge result. FR-007 codifies this no-op invariant.

## §F — Synthetic fixture shape for SC-003

**Decision**: construct a minimal Maven-shaped fixture at `mikebom-cli/tests/fixtures/source_files_union/` that exercises the same-PURL multi-entry shape WITHOUT requiring a real OCI image:

```text
source_files_union/
├── pom.xml                   # declares one dep: pkg:maven/com.example:foo@1.0
├── target/
│   ├── primary.jar           # standalone com.example:foo@1.0.jar at top-level
│   └── fat-bundle.jar        # fat-jar that vendors com.example:foo@1.0 internally
└── README.md                  # documents the shape: one Maven coord, two paths
```

The Maven reader at `scan_fs/package_db/maven.rs:3429-3457` will:
1. Detect `target/primary.jar` as standalone → emit one `PackageDbEntry` for `pkg:maven/com.example:foo@1.0` with `parent_purl = None`.
2. Detect `target/fat-bundle.jar` and walk into it → detect the nested `com.example:foo@1.0` inside → emit one `PackageDbEntry` with `parent_purl = Some(pkg:maven/com.example:fat-bundle@...)`.

Both entries pass through `deduplicate()` (different `parent_purl` → different groups → not merged) and reach the union pass with different single-element `source_file_paths` Vecs. Post-148 the union pass writes the alphabetically-sorted union onto BOTH entries' Vecs.

The integration test runs `mikebom sbom scan --path <fixture> --format <each>` three times (one per format) and asserts that the `mikebom:source-files` value for `pkg:maven/com.example:foo@1.0` is bytewise-identical across all three format outputs (per US1 acceptance scenarios 1+2).

**Alternative considered**:
- **Reuse the polyglot-builder-image fixture from the audit corpus** — rejected: that fixture is a real OCI image, large + slow + opaque. The synthetic fixture is small + fast + intent-documented.

## §G — Golden-refresh scope estimation

**Decision**: existing Maven-bearing CDX/SPDX 2.3/SPDX 3 goldens may experience small `mikebom:source-files` value changes. Specifically: the `pom-three-deps` fixture under `mikebom-cli/tests/fixtures/maven/` may have any same-PURL multi-entry shape (unlikely — it's a simple POM-only fixture); if not, the goldens are unchanged.

**Verification action for Phase 5 (tasks.md)**:
```bash
MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression cdx_regression_maven
MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression maven_byte_identity
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression maven_byte_identity

# Inspect:
git diff --stat -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/maven.*.json
```

**Acceptance**: any drift MUST be limited to:
- (a) `mikebom:source-files` value changes on components that previously carried a non-canonical single-path Vec (Maven nested-coord components specifically).
- (b) New canonical-union Vec contents that include both the standalone path AND the nested path for each affected component.

Reject any drift outside these two patterns (FR-010).

## Summary of decisions feeding Phase 1

- **§A**: Union pass lands at `scan_fs/mod.rs:751` immediately after `deduplicate()`. Order with subsequent passes is not load-bearing (none touch `source_file_paths`).
- **§B**: Implementation lives in `mikebom-cli/src/resolve/deduplicator.rs` as a new `pub fn canonicalize_source_files_by_purl()`. NOT a sibling module.
- **§C**: Idempotence via `BTreeSet<String>` collection type (set-union is idempotent by definition; conversion to sorted Vec is deterministic).
- **§D**: Cross-ecosystem coverage automatic via `Purl::as_str()` keying. Maven is the only ecosystem with confirmed audit findings (51 polyglot-builder-image cases); Cargo workspace + Go vendor cases potentially covered for free.
- **§E**: Existing within-group merge at deduplicator.rs:74-78 stays unchanged. The two passes compose — within-group merge runs first, cross-PURL union writes back over multi-entry results.
- **§F**: Synthetic SC-003 fixture at `mikebom-cli/tests/fixtures/source_files_union/` with one Maven coord appearing both standalone + nested inside a fat-jar.
- **§G**: Golden-refresh scope: 3 maven-bearing goldens (`cyclonedx/maven.cdx.json`, `spdx-2.3/maven.spdx.json`, `spdx-3/maven.spdx3.json`); likely empty diff unless the `pom-three-deps` fixture happens to exercise the multi-entry shape.
- **No new Cargo dependencies.**
- **No new `mikebom:*` annotation** (FR-008 + Constitution V satisfied vacuously).
