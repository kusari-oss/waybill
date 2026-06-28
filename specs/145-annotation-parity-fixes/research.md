# Research — milestone 145 (annotation-emission parity fixes)

Phase 0 output. Resolves the substantive US3 diagnosis question per spec FR-008, plus design decisions for US1 + US2.

## A — `mikebom:file-paths` value-shape fix (US1)

**Decision**: Change line 233 of `mikebom-cli/src/scan_fs/file_tier/mod.rs` from `extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(file_paths_json))` (where `file_paths_json = serde_json::to_string(&paths_str)`) to `extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(paths_str))` directly. Update the unit test at line 405 from `serde_json::from_str(fp)` to direct array iteration via `fp.as_array()`.

**Rationale**:
- The bug is a literal double-encoding: `paths_str` (a `Vec<String>`) gets serialized to a JSON string via `to_string`, then that string is wrapped in a `Value::String` via `json!`. The intent (per the doc-comment at line 190 saying "JSON-encoded-sorted-array") was apparently to encode the array AS a JSON string for the value — but every other array-valued mikebom annotation (`mikebom:source-files`, `mikebom:cpe-candidates`, etc.) emits as a native `Value::Array`. The file-paths emission is the lone outlier; correct alignment is to drop the stringification.
- The `serde_json::to_string` call always succeeds for a `Vec<String>`; the `Ok(...)` branch always fires. There's no defensive value in keeping the `if let Ok(...)` shape after the fix — replacing with a direct `extra_annotations.insert(..., json!(paths_str))` is both shorter and clearer. (Keeping the `if let` defensively would require synthesizing an error case that doesn't exist; the change drops it.)
- Wire-output change is acceptable per spec Assumption 2 (the stringified shape was never documented as the contract; affected only since milestone-133 introduction, ~6 months).
- Test change is from `serde_json::from_str(fp).expect("file-paths JSON parses")` to `fp.as_array().expect("file-paths is array").iter().map(|v| v.as_str().unwrap().to_string()).collect::<Vec<_>>()` or similar. The test ASSERTION already exists; the SHAPE of the parse changes.

**Alternatives considered**:
- **Keep the stringified shape, document it as the contract** — rejected: violates Constitution V's "standards-native > `mikebom:*`" spirit (the native-array shape IS the standard `properties[].value` form), and breaks consumer expectations from the C92 parity-catalog row (`Directionality::SymmetricEqual`).
- **Add a new annotation `mikebom:file-paths-array` with the native shape, deprecate `mikebom:file-paths`** — rejected: doubles wire surface, breaks every existing parity-catalog row, no upside.

## B — `mikebom:lifecycle-scope` SPDX 3 emission (US2)

**Decision**: Add a new emission branch in `mikebom-cli/src/generate/spdx/v3_annotations.rs` that mirrors the SPDX 2.3 sibling at `annotations.rs:227-236`. Specifically, after the existing `mikebom:raw-version` push (around line 263 of v3_annotations.rs), insert:

```rust
// C42 mikebom:lifecycle-scope — parity-bridging annotation mirroring
// the SPDX 2.3 sibling at annotations.rs:227-236 (milestone 145 US2).
// CDX and SPDX 2.3 both emit this annotation for non-Runtime scopes
// (CDX at cyclonedx/builder.rs:851 via `s.is_non_runtime()`, SPDX 2.3
// at annotations.rs:233 via the `LifecycleScope::Runtime => None`
// match arm). SPDX 3 was the pre-145 outlier.
if let Some(ref scope) = c.lifecycle_scope {
    use mikebom_common::resolution::LifecycleScope;
    let s = match scope {
        LifecycleScope::Development => Some("development"),
        LifecycleScope::Build => Some("build"),
        LifecycleScope::Test => Some("test"),
        LifecycleScope::Runtime => None, // runtime is the default; no annotation
    };
    if let Some(s) = s {
        push(out, "mikebom:lifecycle-scope", json!(s));
    }
}
```

**Rationale**:
- Code-confirmed during /speckit-clarify that BOTH peers (CDX + SPDX 2.3) omit Runtime; SPDX 3 must follow the same omission rule (FR-006 + FR-007).
- Insertion point near `raw-version` (C17) lines up with the parity catalog ordering — C42 lifecycle-scope sits in the property-emission block, not in a sibling type.
- Reuses `LifecycleScope::Runtime => None` pattern from `annotations.rs:233` verbatim (single canonical match arm).

**Alternatives considered**:
- **Emit Runtime annotation too** — rejected: violates FR-006/FR-007 and breaks parity with CDX + SPDX 2.3 (would create the inverse problem — SPDX 3 emits an annotation the other formats omit).
- **Add to a dedicated `emit_lifecycle_*` helper** — rejected: there's no precedent for ecosystem-specific lifecycle helpers; inline match is the established pattern.

## C — `mikebom:source-files` per-emitter drift diagnosis (US3) [SC-007]

**Diagnosis** (the spec's required research artifact for SC-007):

The `mikebom:source-files` annotation is emitted from TWO independent sources on every component:

1. **Field-derived source** — `c.evidence.source_file_paths` (a `Vec<String>`), populated at scan-time by the orchestrator from `entry.source_path` via `crate::scan_fs::sbom_path::normalize_sbom_path_relative(&entry.source_path, Some(root))`:
   - **Artefact walker**: `mikebom-cli/src/scan_fs/mod.rs:198` (sets rootfs-normalized path)
   - **Package-DB reader**: `mikebom-cli/src/scan_fs/mod.rs:636` (sets rootfs-normalized path)

   All three SBOM emitters consume this field:
   - CDX: `mikebom-cli/src/generate/cyclonedx/builder.rs:830-839` via `crate::scan_fs::sbom_path::source_files_as_json_array(&component.evidence.source_file_paths)`
   - SPDX 2.3: `mikebom-cli/src/generate/spdx/annotations.rs:302-308` via `json!(c.evidence.source_file_paths)`
   - SPDX 3: `mikebom-cli/src/generate/spdx/v3_annotations.rs:267-273` via `json!(c.evidence.source_file_paths)`

2. **Reader-stamped source** — `c.extra_annotations["mikebom:source-files"]`, stamped DIRECTLY by per-reader code:
   - **Maven reader (nested-JAR path)**: `mikebom-cli/src/scan_fs/package_db/maven.rs:2244` stamps `Value::String(source_files_url.clone())` where `source_files_url` is the `<outer>!<inner>!...` JAR-URL notation (per the line-1312 doc-comment).
   - **Workspace reader**: `mikebom-cli/src/scan_fs/package_db/workspace.rs:76`
   - **Yocto recipe reader**: `mikebom-cli/src/scan_fs/package_db/yocto/recipe.rs:213`

   All three SBOM emitters also iterate `c.extra_annotations` and emit each key→value pair as a property/annotation:
   - CDX: `mikebom-cli/src/generate/cyclonedx/builder.rs:1086-1098` (iterates the bag, stringifies non-String values)
   - SPDX 2.3: `mikebom-cli/src/generate/spdx/annotations.rs:371` (iterates the bag)
   - SPDX 3: `mikebom-cli/src/generate/spdx/v3_annotations.rs:332-337` (iterates the bag)

**The bug**: a Maven component on the `polyglot-builder-image` fixture has BOTH sources stamped:
- `c.evidence.source_file_paths = ["root/.m2/repository/.../surefire-booter-3.2.2.jar"]` (the orchestrator-normalized JAR path)
- `c.extra_annotations["mikebom:source-files"] = Value::String("...!...")` (the Maven-reader-stamped nested-JAR URL string)

Each emitter therefore produces TWO `mikebom:source-files` entries on the same component. The parity-catalog extractor (`mikebom-cli/src/parity/extractors/`) picks ONE per format, and which one it picks (first-match vs last-match, or some other tie-break) differs by emitter implementation — hence the cross-format value drift.

**Why the audit shows a "tempdir" path on SPDX 3 specifically**: the most plausible mechanism, pending fixture-reproduction, is that one of the two emission iterations on the SPDX 3 side (lines 267-273 OR 332-337) is being skipped or overwritten, and the surviving entry happens to be the one whose value resolves to the image-extract tempdir path — likely via an upstream `entry.source_path` value that was NOT normalized (e.g., from a different reader's stamping pass during dedup). The exact dedup-collapse step needs to be reproduced on the fixture during implement phase.

### §C.1 — Confirmation of diagnosis at implement time (T011/T012 outcome)

**Status**: The implement phase confirmed §C's diagnosis via code inspection rather than full fixture reproduction. The four emission sites (Maven reader at `maven.rs:2244` + the three emitter iteration loops at `cyclonedx/builder.rs:1086`, `spdx/annotations.rs:371`, `spdx/v3_annotations.rs:332`) were verified to:

1. **Have the milestone-127 `is_internal_emission_key` filter at all three emitter sites** (verified via `grep -A2 "for (key, value) in &c.extra_annotations" mikebom-cli/src/generate/`), so the defensive Option-1 guard can extend the existing filter pattern uniformly.
2. **Stamp `mikebom:source-files` only at the Maven nested-JAR reader for the polyglot-builder-image case** (workspace + yocto readers also stamp this key but in different contexts not implicated in the audit's 51 Maven-dep cases).

**Decision recorded**: Apply BOTH Option 1 (defensive emitter-side guard) AND Option 2b (Maven reader renames its stamped key to `mikebom:source-files-nested-url`). Defense-in-depth: Option 2b removes the trap at the source; Option 1 catches any future readers tempted to recreate the same trap. Both fixes restore C18 `SymmetricEqual` parity.

**Full fixture reproduction on `polyglot-builder-image` is operator-cadence** (per research §D) and will be verified via the harness re-run documented in tasks.md T023. The in-tree integration test (T016) provides the CI-binding signal that the dedup invariant holds for the synthetic Maven double-stamp case.

---

**Decision** (original): Two-part fix.

1. **Suppress double-emission**: At all three emitter sites (CDX builder.rs:1086, SPDX 2.3 annotations.rs:371, SPDX 3 v3_annotations.rs:332), add a guard that SKIPS keys ALREADY emitted from the field-derived source. The simplest implementation is a set of "field-owned keys" — `["mikebom:source-files"]` at minimum, possibly extended to other dual-source keys as audit reveals them — checked in the iteration loop. The field-derived source wins because it goes through the milestone-133 normalization pipeline (`normalize_sbom_path_relative`), guaranteeing consistent rootfs-relative paths.

2. **Maven reader: stop stamping `mikebom:source-files` into `extra_annotations`** for the nested-JAR-URL case. The Maven-reader stamping at `maven.rs:2244` was introduced when the field-derived source did NOT yet carry nested-JAR provenance; milestone-133's normalization pipeline + the field's `Vec<String>` shape now subsume the use case. Drop the stamping, OR keep it under a DIFFERENT annotation key (e.g., `mikebom:source-files-nested-url`) that doesn't collide with the field-derived emission.

The implement phase will reproduce the drift on the `polyglot-builder-image` fixture to choose between (1) and (2) — (1) is purely defensive at the emission boundary; (2) eliminates the duplicate-source at the population boundary, which is structurally cleaner but has a wider blast radius (must verify no consumer depends on the Maven-stamped value at its current key).

**Rationale**:
- Both fixes preserve the operator-visible information (field-derived path; the JAR-URL `<outer>!<inner>!` notation is still queryable if option 2 keeps it under a separate key).
- Both fixes restore C18 (`mikebom:source-files`) `SymmetricEqual` parity across all three formats.
- Option 1 is contained and low-risk; option 2 is the long-term-correct fix that removes the trap for future contributors.
- Defensive doc-comments at the two stamping sites (the field setter and the Maven reader) noting the dual-emission risk would prevent recurrence.

**Alternatives considered**:
- **Canonicalize on the Maven-stamped value, drop the field-derived emission for Maven components** — rejected: the field-derived value is normalized via the milestone-133 pipeline (guarantees rootfs-relative paths), which is what the parity-catalog C18 row expects. Dropping it would break other ecosystem readers' emissions that rely on the normalized field.
- **Merge both values into a single array on emission** — rejected: produces a non-canonical mixed-shape value (some entries are rootfs-relative paths, some are JAR-URL `!` notation), which would break downstream consumers that parse the value as a path list.

## D — Test strategy for SC-001 / SC-004 / SC-008 (harness-finding counts)

**Decision**: In-tree tests provide CI-independent guards on the underlying code behavior. The harness-reported finding counts (SC-001 / SC-004 / SC-008) are verified separately by re-running the sbom-conformance harness — typically out of CI's loop, on operator-controlled cadence.

**Rationale**:
- The sbom-conformance harness is an external Go binary; depending on it as a CI dep would violate Constitution principles around test isolation + reproducibility.
- The in-tree tests (SC-002, SC-005, SC-009) assert on the CODE-LEVEL invariants (value-type checks, annotation presence, byte-equivalent emission). If those pass, the harness-finding counts will fall accordingly. The exception is US3's exact 51-count match, which depends on the polyglot-builder-image fixture's specific component count; the in-tree test asserts on the dedup invariant directly (any two components with the same logical purl emit the same `mikebom:source-files` value across formats).
- Re-running the harness post-merge is documented in `quickstart.md` as the operator verification step for SC-001/SC-004/SC-008.

**Alternatives considered**:
- **Vendor the harness as a CI dep** — rejected: adds a Go toolchain to mikebom CI; out of scope for this milestone.
- **Skip harness verification entirely** — rejected: SC-001/SC-004/SC-008 explicitly call out the harness-finding counts as the outcomes.

## E — Golden-fixture refresh scope (FR-010)

**Decision**: For US1 (`mikebom:file-paths` shape change), refresh every existing golden file containing the pre-145 stringified-array shape. Find via `grep -rln 'mikebom:file-paths' mikebom-cli/tests/fixtures/golden/`. Diffs are limited to ONE pattern per affected line: `"value": "[..."` → `"value": [...`. For US2 (SPDX 3 lifecycle-scope addition), refresh only SPDX 3 goldens for fixtures that contain non-Runtime-scoped components (likely `node-dev-vs-prod` and similar). For US3, the fix MAY produce golden diffs if the affected fixtures' Maven components are captured in goldens.

**Rationale**:
- The shape change in US1 is byte-level — `serde_json::to_string` produces different bytes for a `Value::String("[\"x\"]")` than for a `Value::Array([String("x")])`. Goldens must reflect the post-fix byte sequence.
- US2 is purely additive — CDX and SPDX 2.3 goldens DO NOT need refresh (those emit lifecycle-scope today). Only SPDX 3 goldens for fixtures with non-Runtime scope.
- US3's golden refresh scope depends on which fix path is chosen (the implement-phase reproduction will reveal this).

**Refresh env vars** (matching the milestone-144 pattern):
- `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression`
- `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression`
- `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression`

## F — Why no `Cargo.toml` dependency changes?

All three fixes use existing types from the workspace dep closure:
- `serde_json::Value::Array` / `Value::String` distinction — already pervasive.
- `mikebom_common::resolution::LifecycleScope` — milestone-049 type; reused verbatim.
- `BTreeMap<String, Value>` for `extra_annotations` — milestone-023 introduction; unchanged.

No new crates needed.

## Summary of decisions feeding Phase 1

- **US1**: 1-line constructor fix at `file_tier/mod.rs:233` + 1-test-update at line 405 + golden refresh.
- **US2**: 1-new-emission-branch in `v3_annotations.rs` mirroring `annotations.rs:227-236` + 2 unit tests + SPDX 3 golden refresh (subset).
- **US3**: implement-phase reproduces the drift on the polyglot-builder-image fixture and applies either (option 1) defensive emitter-side guard against double-emission, OR (option 2) Maven-reader-side stop stamping `mikebom:source-files` into `extra_annotations`. Defensive doc-comments at both stamping sites prevent recurrence.
- **Test strategy**: in-tree tests are the CI-binding signal; harness re-run is the operator-verification step.
- **No new Cargo dependencies.**
