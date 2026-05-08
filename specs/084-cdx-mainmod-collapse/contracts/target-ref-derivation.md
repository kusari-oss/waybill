# Contract — milestone 084 `target_ref` derivation + CDX ref closure invariant

The milestone's only contract. Documents the `target_ref` derivation rule + the closure-invariant assertion + the relationships-filter rule for the override path.

## CLI surface

**No new operator-facing CLI flags.** This is an internal correctness fix. No flags added, removed, or repurposed. Existing flags (`--root-component-name`, `--root-component-version`, `--format cyclonedx-1-6`, etc.) keep their semantics.

## Library surface (`mikebom-cli` crate)

**No new public Rust API.** The fix changes a local `let` binding inside a private builder method (`cyclonedx/builder.rs::build_cdx_1_6`); it does not cross any module boundary that downstream code depends on.

## Identifier derivation contract

### `target_ref` derivation (the milestone's load-bearing rule)

For every CDX 1.6 emission, the local `target_ref: String` value computed in `cyclonedx/builder.rs::build_cdx_1_6` MUST be derived per:

```text
                     ┌─────────────────────────────────────────────────────────┐
                     │                                                         │
   override_active   │                  main-module-detected                   │ target_ref =
                     │                                                         │
─────────────────────┼─────────────────────────────────────────────────────────┼─────────────────
                     │                                                         │
       false         │                          true                           │ <main-module PURL>
                     │                                                         │
       false         │                         false                           │ "{name}@{version}"
                     │                                                         │
       true          │                       (any)                             │ "{name}@{version}"
                     │                                                         │
                     │   (where {name}+{version} are override values when      │
                     │    override is active, else effective_target_*)         │
                     └─────────────────────────────────────────────────────────┘
```

`main-module-detected` ≡ at least one component in `effective_components` has `extra_annotations["mikebom:component-role"].as_str() == Some("main-module")`. (Same predicate as the override-filter at `cyclonedx/builder.rs:272-293`.)

This contract is enforced by VR-084-001 + VR-084-005 (data-model.md).

### Relationship filtering under override

When `override_active`, the `relationships` slice passed to `build_dependencies` MUST be filtered so that no edge has `from` equal to the dropped main-module PURL. Edges with `from = main-module-PURL` are either:

- **Rewritten**: `from` replaced with the override-form `target_ref`, preserving the `to` and `relationship_type` — keeps the dep-tree shape under override.
- **Dropped**: if rewriting would produce a duplicate already in `relationships[]`.

Implementation lives at the same site as the existing component-filter (`cyclonedx/builder.rs:272-293`), so the dropped main-module PURL is captured once and reused for relationship filtering.

This contract is enforced by VR-084-006 (data-model.md) and exercised by the milestone-077 fixture's regenerated golden plus the new closure-invariant regression test.

## Closure invariant contract

### Definition

```text
S = components[].bom-ref ∪ {metadata.component.bom-ref}
```

`S` is the set of all bom-ref values *declared* in the document.

### Invariant (the spec contract from CDX 1.6 `refLinkType`)

For every emitted CDX 1.6 document:

```text
dependencies[].ref           ⊆  S
dependencies[].dependsOn[]   ⊆  S
compositions[].assemblies[]  ⊆  S
compositions[].dependencies[] ⊆  S
```

Equivalently: every value in any `refLinkType`-typed field MUST be a key the document defines.

This contract is enforced by VR-084-002 + asserted operationally by the new regression test at `mikebom-cli/tests/cdx_ref_closure_invariant.rs` per research §4.

### Test invocation contract

```bash
cargo +stable test -p mikebom --test cdx_ref_closure_invariant
```

Output: `0 failed`; per-fixture violations (if any) print with full ref values + the field path that contained them, so a future regression's diagnosis is one log-read away.

The test is hermetic — no external tool (no `cyclonedx-cli` shell-out, no internet). Runs under `cargo test --workspace` in the existing CI lanes.

## Per-format scope contract

| Format | Affected? | Verification |
|---|---|---|
| **CDX 1.6** | YES — the bug + the fix | New closure-invariant test + golden regen |
| **SPDX 2.3** | NO — different identifier conventions | `cargo +stable test -p mikebom --test spdx_regression` byte-identical pre/post |
| **SPDX 3 (3.0.1)** | NO — different identifier conventions | `cargo +stable test -p mikebom --test spdx3_regression` byte-identical pre/post |

VR-084-007 (data-model.md) makes SPDX byte-identity a hard gate.

## Pre-PR gate contract

The standard CLAUDE.md two-command gate, plus milestone-specific verification:

```bash
# Standard gate
./scripts/pre-pr.sh

# Milestone-specific
cargo +stable test -p mikebom --test cdx_ref_closure_invariant -- --nocapture
cargo +stable test -p mikebom --test spdx_regression
cargo +stable test -p mikebom --test spdx3_regression
```

All four MUST be `0 failed`. Pre-PR script clean (zero clippy warnings, all suites pass). The milestone-specific commands are also exercised inside `cargo test --workspace`; they are listed separately for explicit pre-PR inspection.

## Performance contract

- Emission wall-time: byte-identical to milestone-082 baseline (the fix removes one `dependencies[]` entry construction + retargets two strings; net change ≤ −1 allocation per emission).
- Test wall-time: new closure-invariant test runs in <2s end-to-end (8 fixtures × ~250ms emission + ~10ms set check).
- Goldens regen wall-time: <30s for ~6-8 affected goldens.

## Backward-compatibility contract

- Operators of mikebom-emitted CDX 1.6 documents post-fix observe one fewer `dependencies[]` entry + two retargeted `compositions[]` entries vs alpha.23. No other field changes. Any consumer that previously relied on the orphan ref as a load-bearing identifier was already broken; the fix is observable, never silent.
- Operators of SPDX 2.3 or SPDX 3 outputs see no diff (FR-007).
- Operators using `--root-component-name` override observe no `metadata.component`-level change; their override identity continues to be the document subject. The override path's `dependencies[]` may show one fewer entry (the dropped main-module PURL bridge entry per research §2 Option A).
