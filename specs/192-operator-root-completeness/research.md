# Research: m192 Operator-Root Graph-Completeness Fix

**Date**: 2026-07-14
**Purpose**: Verify the exact fix location + confirm the "operator-override" detection primitives + audit for byte-identity risk.

## R1 — Fix location + call-site plumbing

**Decision**: Extend `build_ecosystem_root_set` at `mikebom-cli/src/generate/graph_completeness/bfs.rs:73` to accept a new `target_ref: &str` parameter. Add a synthesis pass that runs AFTER the existing per-ecosystem-root population loop, only when the primary `RootSelectionResult.subject` variant is NOT `MainModule`. Update the sole caller at `mikebom-cli/src/generate/graph_completeness/mod.rs:156` to thread through the `target_ref` value that's already available in scope.

**Rationale**: Verified via code inspection:

- `mod.rs:156` — `let mut root_set = bfs::build_ecosystem_root_set(components, selection);` — the single call site. `target_ref` is already a `&str` parameter to `compute_graph_completeness` at line 145, so threading it through is a one-line change.
- `bfs.rs:107-116` — the existing per-ecosystem loop already populates `per_ecosystem_root` for detected main-module components. The synthesis pass adds a parallel path for operator-override scans: for every ecosystem in `components[]`, insert `(ecosystem, target_ref.to_string())` into `per_ecosystem_root` unless that ecosystem already has an entry (from a native main-module) OR unless that ecosystem matches the operator's chosen root PURL type (per Q2).
- `bfs.rs:120-127` — `ecosystems_without_root` is computed by filtering `components[]` ecosystems by `!per_ecosystem_root.contains_key(e)`. Post-synthesis, every ecosystem present in `components[]` has an entry, so `ecosystems_without_root` is empty for operator-override scans.

**Alternatives considered**:
- (A) Fix at the classifier site (`mod.rs:229-253`) by skipping the `MultiEcosystemPartialRoot` push for operator-override scans: rejected — leaves `ecosystems_without_root` non-empty in the returned `EcosystemRootSet`, which other future consumers might misinterpret. Better to fix the source.
- (B) Change `RootSelectionResult` to include a synthetic per-ecosystem root list at selection time: rejected — widens the API surface of the root selector for a fix that's local to graph-completeness. Selection semantics are unchanged; only completeness classification uses this distinction.
- (C — **CHOSEN**) Extend `build_ecosystem_root_set` with the synthesis pass, thread `target_ref` through. Minimal API delta; localizes the fix to the module that owns the classifier.

## R2 — Operator-override detection (per Q2 answer A)

**Decision**: Detect "operator-picked ecosystem" by parsing the `target_ref` string via `mikebom_common::types::purl::Purl::new(target_ref)`. If parse succeeds AND the returned `Purl.ecosystem()` is anything other than `"generic"`, treat that ecosystem as "already covered by the operator's root PURL" and SKIP synthesis for it. If parse FAILS (e.g., legacy pre-m084 `<name>@<version>` short-form), treat the root as generic — synthesize for every ecosystem in `components[]`.

**Rationale**: `Purl::new` at `mikebom-common/src/types/purl.rs:6` is spec-strict; the `ecosystem()` method at line 141 returns the type segment. This gives us a self-contained check that reads exactly what the operator wrote (via `--root-purl-type` if set, else defaults to `generic`). Fallback-to-generic on parse failure matches the pre-m084 shape (`target_ref = "name@version"` without a `pkg:` prefix) — those scans have no ecosystem intent, so synthesize for every ecosystem in components.

**Alternatives considered**:
- (A) Dispatch on `ResolvedRootSubject` enum variants: rejected — couples the fix to enum-variant internals; future variant additions could silently break the fix.
- (B) Detect via CLI flag inspection (whether `--root-purl-type` was passed): rejected — cross-crate flag-plumbing for a signal that's already visible in the target_ref string itself.

**References**:
- `mikebom-common/src/types/purl.rs:141` — `Purl::ecosystem()` accessor.
- `mikebom-cli/src/generate/cyclonedx/builder.rs:446-449` — target_ref construction.
- Q2 answer A recorded in `spec.md::Clarifications::Session 2026-07-14`.

## R3 — Byte-identity risk audit (native-root path)

**Decision**: The fix's synthesis path guards on `!matches!(selection.subject, ResolvedRootSubject::MainModule(_))`. When `MainModule` is the subject (any native-root scan — Go module detected, npm workspace root picked, etc.), synthesis is a no-op and `build_ecosystem_root_set` produces byte-identical output to pre-m192. Every existing golden that was generated from a native-root scan MUST pass byte-identically post-m192.

**Rationale**: Verified via test-shape audit:
- `mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.json`: the npm workspace fixture has a Go main-module... wait, it's npm — so `mikebom:component-role: main-module` on an npm package.json. That's a `MainModule` selection subject. Byte-identity preserved.
- `mikebom-cli/tests/fixtures/golden/cyclonedx/cargo.cdx.json`, `golang.cdx.json`, `maven.cdx.json`, `gem.cdx.json`, `pip.cdx.json`: same — all fixture repos have detectable main-module components, so `MainModule` selection subject. Byte-identity preserved.
- OS-image fixtures (`apk.cdx.json`, `deb.cdx.json`, `rpm.cdx.json`, `bazel.cdx.json`, `cmake.cdx.json`): the root selector picks `SyntheticPlaceholder` for these (no detectable single main-module). Post-fix synthesis WOULD fire for these scans BUT — verify: are those fixtures currently reporting `complete` or `partial`?

If they currently report `partial: multi-ecosystem-partial-root: <ecosystem>`, the fix flips them to `complete` — this IS a golden diff. If they currently report `complete` already (because `orphan_count == 0` naturally), the fix is a no-op (no classifier firing to suppress).

**Investigation task**: at T003 in tasks.md, grep every golden for `"mikebom:graph-completeness"` values. Any golden with `partial: multi-ecosystem-partial-root` needs regen; any with `complete` is unaffected. This is the drift set for FR-011.

## R4 — Observability contract per FR-009 / Q1

**Decision**: Emit exactly one `tracing::info!` at the end of `build_ecosystem_root_set` when the synthesis path fired, with structured fields:

```rust
tracing::info!(
    synthesized_ecosystems_count = synthesized_count,
    "synthesized per-ecosystem placeholder roots for operator-override scan"
);
```

Placed inside the synthesis conditional so it fires ONLY when synthesis actually happened. `synthesized_count = 0` means synthesis path didn't execute (native-root case OR operator-override with zero ecosystems in components) — no log line emitted in that case.

**Rationale**: Structured tracing fields match m173/m158 convention. Grep-friendly for CI-log analysis. `RUST_LOG=info` (mikebom's default level) shows it by default per Q1 answer A.

## R5 — Emitter-side interaction

**Decision**: No emitter-side change required. The `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` annotations are emitted from `GraphCompletenessResult.value` + `.reason_codes` respectively; both flow through unchanged. All three format emitters (CDX, SPDX 2.3, SPDX 3) consume the same `GraphCompletenessResult` per the m158 wiring; the fix affects only what value ends up in that struct.

**Rationale**: Verified — the classifier result is computed ONCE at emit time (`builder.rs:487-494`) and passed to all three format emitters. Post-fix, all three formats reflect the corrected value identically.

## R6 — Native-first Principle V audit

**Decision**: NO new `mikebom:*` annotation is introduced. The existing `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` annotations (established by m158) carry all the needed signal; this milestone corrects the VALUES emitted into those channels. Per FR-008, no new fields.

**Rationale**: Direct audit satisfying Principle V. Recorded in FR-008.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Fix location | `bfs.rs::build_ecosystem_root_set` extension | classifier site; RootSelectionResult widen | Fixes the source, not the symptom; minimal API delta |
| Operator-eco detection | Parse `target_ref` via `Purl::new`, read `.ecosystem()` | ResolvedRootSubject enum dispatch; CLI-flag inspection | Reads what the operator wrote; future-variant-proof |
| Fallback on Purl parse failure | Treat as generic → synthesize for every ecosystem | Skip synthesis entirely | Legacy pre-m084 `name@version` shape must still get the fix |
| Byte-identity gate for goldens | Native-root fixtures unchanged | Regenerate all | MainModule path is a no-op; only OS-image fixtures may drift |
| Observability | INFO tracing summary via structured fields | DEBUG-only; document annotation | Per Q1; matches m158 convention |
| Emitter-side change | None (classifier-input fix only) | Per-format value override | Result flows through existing channels unchanged |
| New Cargo deps | None | Add regex-based PURL parser | Existing Purl newtype already parses |
| New `mikebom:*` annotations | None | Add `mikebom:completeness-synthesis-applied` | Existing annotations carry the signal |
