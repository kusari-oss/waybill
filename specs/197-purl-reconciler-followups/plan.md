# Implementation Plan: m190 + m191 Follow-Up Bundle

**Branch**: `197-purl-reconciler-followups` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/197-purl-reconciler-followups/spec.md`

## Summary

Bundle of 7 targeted follow-up items to m190 (epoch PURL emission) and
m191 (design-tier / source-tier reconciler): (a) US1/US2/US2b — audit
dpkg / apk / rpm readers for epoch `?epoch=<N>` qualifier emission,
mirroring the m190 opkg fix pattern (`ipk_file.rs::parse_opkg_version_with_epoch`
+ `build_opkg_purl`); (b) US3 — extend the m191 versionless-PURL fix
to 6 additional ecosystem readers (composer / dart / cocoapods / scala
/ haskell / erlang); (c) US4 — add a fuzz-style round-trip test over
all 11 ecosystems with ≥ 100 synthetic inputs each; (d) US5 — extend
the m191 reconciler to recognize npm-alias declarations and preserve
the original alias as a `mikebom:declared-as` annotation; (e) US6 —
convert reconciler declaration-provenance annotations from singular
scalars to always-array shape per Q1 clarification. Zero new Cargo
dependencies. Existing reader / reconciler / test-harness patterns
extended in place.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–196; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — the `mikebom_common::types::purl::Purl` type (m005+; reused by all 11 emitters), the m191 reconciler at `mikebom-cli/src/resolve/reconciler.rs`, the m190 opkg-side epoch helpers at `mikebom-cli/src/scan_fs/package_db/ipk_file.rs` (`parse_opkg_version_with_epoch` + `build_opkg_purl(..., epoch)` — the pattern US1/US2/US2b copy). `serde_json` for annotation value construction (already pervasive). No new crates. Fuzz test uses a hand-rolled catalog-driven generator per spec Assumption 3 — NOT `proptest` or `quickcheck`.
**Storage**: N/A — all state is in-process per scan; matches every reader milestone since 002.
**Testing**: `cargo test --workspace` runs new unit tests (per-reader epoch handling, per-ecosystem versionless-PURL construction, npm-alias detection, reconciler always-array shape) + the new fuzz suite (`cargo test -p mikebom-common versionless_purl_fuzz`). Golden regen scope: subset of m191 reconciler-path goldens per Q1 clarification exception (FR-007 amended).
**Target Platform**: Same as mikebom itself — Linux + macOS host, targeting the same output SBOM shape.
**Project Type**: Reader-and-reconciler augmentation. Adds ~500-1000 LOC across 3 epoch-reader touch-ups + 6 versionless-PURL touch-ups + 1 fuzz test + 1 reconciler-shape refactor + 1 alias-plumbing pass. No new files under `src/`; all changes edit existing readers + the reconciler.
**Performance Goals**: No perf-regression budget beyond FR-008 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s vs pre-m197 baseline per SC-007). The fuzz test's 1100+ invocations MUST complete in under 5s on a warm cargo cache — hand-rolled generator + `Purl::new` are both O(1) per invocation.
**Constraints**: (a) zero new Cargo deps; (b) FR-007 additive-only guarantee with the single Q1 exception on reconciler survivor field shape; (c) all 6 pre-existing GH follow-up issues (#562, #563, #564, #565, #566, #567) closed by the m197 PR per SC-001; (d) US2b rpm audit shipped in-milestone even without a pre-existing GH issue.
**Scale/Scope**: 7 user stories → ~10 edited source files + ~5 new test files + ~5-15 golden regen (m191 reconciler-path exercises only).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All work stays in Rust. Fuzz generator is stdlib-only.
- **II. eBPF-Only Observation** — ✅ N/A.
- **III. Fail Closed** — ✅ PASS. Epoch-parse failures return `Result<>` and propagate (no silent inline embedding).
- **IV. Type-Driven Correctness** — ✅ PASS. Epoch is `Option<i64>` per the m190 opkg pattern; reconciler survivor arrays typed as `Vec<String>`. No stringly-typed shapes.
- **V. Specification Compliance** — ✅ PASS. `?epoch=` is a purl-spec-blessed qualifier. `mikebom:declared-as` is a new `mikebom:*` annotation — audit for native alternatives (CDX 1.6 has no first-class "alias" field; SPDX 3 has no `LifecycleScope::alias`; nearest neighbor is CDX `evidence.identity.methods[]` which is for identification-evidence not aliasing). No native construct carries the semantic — a `mikebom:*` extension is the correct move.
- **VI. Three-Crate Architecture** — ✅ PASS. All edits stay in `mikebom-cli` + one edit to `mikebom-common::Purl` doctest coverage for the fuzz test. No changes to `mikebom-ebpf`.
- **VII. Test Isolation** — ✅ PASS. Every new fixture is `tempfile::tempdir()`-scoped.
- **VIII. Completeness** — ✅ PASS. Epoch fix closes a false-negative-vuln-lookup gap; versionless-PURL fix closes 6 ecosystems' purl-spec-non-conformance gap; reconciler improvements close npm-alias + monorepo-declaration provenance gaps.
- **IX. Accuracy** — ✅ PASS. Fuzz test explicitly checks byte-identity round-trip; epoch qualifier form is verified against purl-spec canonical.
- **X. Transparency** — ✅ PASS. New annotation `mikebom:declared-as` is transparency about the alias-vs-resolved-identity mapping; array-shape rotation is documented in FR-006 with migration note.
- **XI. / XII. Enrichment** — ✅ N/A.
- **Strict Boundary §5 (file-tier)** — ✅ PASS. No file-tier changes.

**Result**: All principles PASS. No violations. New `mikebom:declared-as` annotation audit passed (no native construct available; spec-compliant extension).

**Post-Phase-1 re-check**: N/A here — Phase 1 introduces no new entities beyond what the spec's Key Entities section already documented (epoch-qualified PURL / versionless PURL / reconciler survivor / npm alias declaration). All 4 are already covered by existing type infrastructure (Purl newtype, extra_annotations map). Constitution gate trivially remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/197-purl-reconciler-followups/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 6 investigation decisions
├── data-model.md        # Phase 1 output — annotation-shape enumeration + reconciler-flow diagram
├── quickstart.md        # Phase 1 output — per-US repro steps
├── contracts/
│   └── annotation-shapes.md   # NEW — always-array field shapes + `mikebom:declared-as` schema
├── checklists/
│   └── requirements.md
├── scratch/             # Empirical audit findings (rpm audit result, drift set, etc.)
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/
├── dpkg.rs                        # MODIFIED — US1: add `?epoch=` qualifier emission (mirror ipk_file.rs pattern)
├── apk.rs                         # MODIFIED — US2: same pattern for apk
├── rpm.rs                         # AUDITED — US2b: verify existing epoch handling; add non-regression fixture (likely already correct)
├── composer.rs                    # MODIFIED — US3: versionless-PURL fix
├── dart.rs                        # MODIFIED — US3
├── cocoapods.rs                   # MODIFIED — US3
├── scala.rs                       # MODIFIED — US3
├── haskell.rs                     # MODIFIED — US3
├── erlang.rs                      # MODIFIED — US3
└── npm/
    └── alias_mapping.rs           # MODIFIED — US5: expose alias-source (raw declared name) to the reconciler

mikebom-cli/src/resolve/
└── reconciler.rs                  # MODIFIED — US5 + US6:
                                   #   - always-array shape (US6)
                                   #   - npm-alias resolved-identity matching + mikebom:declared-as (US5)

mikebom-common/src/types/purl.rs   # POSSIBLY MODIFIED — audit whether Purl newtype's versionless-serialization has
                                   # per-ecosystem quirks the fuzz test surfaces (Q1 already committed to
                                   # per-ecosystem PURL construction happening in the readers, not the Purl type)

mikebom-common/tests/
└── versionless_purl_fuzz.rs       # NEW — US4: hand-rolled catalog-driven generator, 100+ inputs per ecosystem

mikebom-cli/tests/
├── fixtures/                      # NEW fixtures under existing test-fixture directories:
│   ├── dpkg/epoch/                # Synthetic .deb with epoch (US1 verification)
│   ├── apk/epoch/                 # Synthetic .apk with epoch (US2)
│   ├── rpm/epoch/                 # Synthetic .rpm with epoch (US2b non-regression)
│   ├── composer/versionless/      # composer.json declaring versionless dep (US3)
│   ├── ...                        # (same shape for dart/cocoapods/scala/haskell/erlang)
│   ├── npm/alias/                 # package.json with `"my-alias": "npm:actual@1.0.0"` (US5)
│   └── npm/multi-declaration/     # monorepo shape with 2 sibling manifests, same dep (US6)
└── fixtures/golden/               # SUBSET REGENERATED — m191 reconciler-path goldens rotate to always-array shape
```

**Structure Decision**: In-place augmentation of 10 existing reader / reconciler source files + 1 new test file (`versionless_purl_fuzz.rs`). No new library modules, no new test binaries beyond the fuzz test file. Golden regen scope is bounded to the m191 reconciler-path exercises per the Q1 exception carved into FR-007.

## Complexity Tracking

No constitution violations. All principles pass on first check. `mikebom:declared-as` audit noted in Principle V confirms no native construct available; extension is the correct move.
