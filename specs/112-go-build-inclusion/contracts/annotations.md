# Contract: Build-Inclusion Annotations & Format Mapping

**Feature**: 112-go-build-inclusion

## Annotation keys

### `mikebom:build-inclusion` (NEW)

Open-enum string. Values this milestone: `unknown`, `not-needed`.
Backed by the typed `BuildInclusion` enum (Constitution IV); attached
only to golang source-tier components.

**Native-construct audit (Constitution V bullet 5 — cited in spec FR-001/FR-004)**:

- `unknown`: no native construct in any target format. CDX `scope`
  (`required|optional|excluded`) cannot express "undetermined"
  (`optional` asserts a positive fact); CDX `evidence`/`confidence`
  covers identity, not build inclusion. SPDX 2.3 has no per-package
  inclusion field. SPDX 3 `LifecycleScopeType` has no unknown value;
  `RelationshipCompleteness` qualifies relationship sets, not
  components. → custom property justified.
- `not-needed`: CDX native `scope: "excluded"` exists and is the
  PRIMARY signal; the property carries the finer-grained reason. SPDX
  2.3 and SPDX 3 lack an excluded-scope construct → annotation is the
  parity bridge (justification clause to be recorded in
  `docs/reference/sbom-format-mapping.md`, naming the missing native
  excluded-scope field).

### `mikebom:build-inclusion-derivation` (NEW)

Open-enum string. Value this milestone: `go-mod-why`. Required
companion of `not-needed` (provenance discriminator, Constitution X).

### `mikebom:lifecycle-scope-derivation` (EXISTING key, new value)

Gains value `go-mod-why` alongside PR #332's `test-only-closure`, on
modules test-tagged by package-level analysis.

## Per-format emission

| Status | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| Unknown | component property `mikebom:build-inclusion` = `unknown`; NO `scope` field (consumer default = required, FR-002) | Package annotation (existing `annotations.rs` bag path) | Element annotation (existing `v3_annotations.rs` bag path) |
| NotNeeded | `scope: "excluded"` (unconditional — independent of `--include-dev`/`--exclude-scope`; bypasses the gate at `builder.rs:599–605`) + properties `mikebom:build-inclusion` = `not-needed`, `mikebom:build-inclusion-derivation` = `go-mod-why` | annotations, same keys/values; dependency relationship stays plain `DEPENDS_ON` (no DEV/BUILD/TEST relationship is asserted — none is true) | annotations, same keys/values; `dependsOn` relationship stays unscoped |
| TestOnly (via toolchain) | existing path: `scope: "excluded"` + `mikebom:lifecycle-scope` = `test`, plus `mikebom:lifecycle-scope-derivation` = `go-mod-why` | existing native `TEST_DEPENDENCY_OF` (full mode) + existing C42 annotation | existing native `LifecycleScopedRelationship` scope `test` |

## Set semantics

- `Unknown` and `NotNeeded` components are NEVER dropped from output,
  under any flag combination (clarification 2026-06-11).
- Toolchain test-tagged components follow the PRE-EXISTING test-scope
  drop semantics (dropped unless `--include-dev`; post-052
  `--exclude-scope` machinery) — no new behavior.
- No pass adds or removes components (FR-011).

## Parity catalog (docs/reference/sbom-format-mapping.md)

Two new rows (auto-parsed by `parity/catalog.rs`); next free row ids at
implementation time (C-series):

1. `mikebom:build-inclusion` — property (open-enum: `unknown` /
   `not-needed`); CDX: property (+ native `scope: excluded` for
   not-needed); SPDX 2.3: annotation (bridge — no native excluded
   scope); SPDX 3: annotation (bridge — `LifecycleScopeType` has no
   excluded/unknown value).
2. `mikebom:build-inclusion-derivation` — property (open-enum:
   `go-mod-why`); same carriage in all three formats.

Plus: amend the existing `mikebom:lifecycle-scope-derivation` row's
value enumeration with `go-mod-why`.

## Byte-identity envelope (FR-008 / SC-004)

- Components with `build_inclusion == None` and no toolchain verdict
  emit byte-identical pre-feature JSON.
- Non-Go goldens: zero drift.
- Go goldens: regenerate once
  (`MIKEBOM_UPDATE_CDX_GOLDENS=1` / `MIKEBOM_UPDATE_SPDX_GOLDENS=1`);
  expected drift limited to the rows above on fallback-discovered
  fixture components.
- The full test suite runs with `MIKEBOM_NO_GO_MOD_WHY=1` (shared test
  env helper) so golden content is host-toolchain-independent.
