# Implementation Plan: Pants JavaScript/npm corpus regression gate

**Branch**: `675-pants-js-corpus` | **Date**: 2026-09-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/675-pants-js-corpus/spec.md`

## Summary

Add a single Pants-managed JavaScript monorepo target to the m195 public corpus harness — as a permanent regression gate against silent breakage of the existing npm reader stack on real-world Pants-JS deployments. Zero waybill production code changes. MVP scope is npm-only (`package-lock.json`); pnpm and yarn coverage are follow-up issues. Fixture selection is a planning-phase survey with a synthetic-fallback escape hatch: if no small/stable public Pants-JS monorepo exists, a synthetic fixture in `waybill-test-fixtures` is an acceptable substitute per the Session 2026-09-02 clarification. Layer 2 goldens are scoped to the `pkg:npm/*` surface only, not the full SBOM — this keeps them under 500 KB and isolates the regression signal from unrelated ecosystem drift in the pinned fixture.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–674; no nightly required for user-space test infra).

**Primary Dependencies**: Existing only — `serde_json` (existing corpus-harness JSON parsing), `tempfile` (per-test scratch dirs), `regex` (existing masking helpers). **Zero new Cargo dependencies at the workspace level** (SC-001). Runtime externals: `git` (already an implicit project assumption — used by the existing m090 fixture cache, m195 corpus cache, m053 golang reader).

**Storage**: N/A for waybill production code. Corpus cache at `~/.cache/waybill/corpus/<source-id>/<pinned-sha>/` — existing m195 layout, unchanged. Goldens at `waybill-cli/tests/fixtures/public_corpus/<target-name>/{cdx,spdx-2.3,spdx-3}.json` — existing m196 golden layout, unchanged (though contents are JS-filtered per FR-008).

**Testing**: `cargo test --test public_corpus`. New target's `#[test]` function follows the existing `run_target(name)` helper pattern. Gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` per FR-007. Golden regen via `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`.

**Target Platform**: Nightly CI on `ubuntu-latest` (the existing corpus lane). Local dev on macOS x86_64/aarch64 is also supported. Zero new platform-specific code paths.

**Project Type**: Single-project (waybill-cli tests). No new modules; the change surface is:
- 1 new `CorpusTarget` entry in `waybill-cli/tests/corpus_harness_195/manifest.rs`
- 1 new layer 1 assertion function in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs`
- 1 new `#[test]` entry in `waybill-cli/tests/public_corpus.rs`
- 1 new JS-filter helper in `waybill-cli/tests/corpus_harness_195/layer2_golden.rs` (or a new adjacent module) — this is the novel piece
- 3 new golden files under `waybill-cli/tests/fixtures/public_corpus/<target-name>/`

**Performance Goals**: SC-005 — nightly-lane runtime attributable to this target ≤ 60s end-to-end (clone + scan + assertion + golden compare). The existing 6 corpus targets each average ~30-90s; a Pants-JS monorepo of the size the assumptions call for (tens-to-hundreds of npm components) fits comfortably.

**Constraints**:
- SC-004: golden fixture size (all three formats combined, JS-filtered) ≤ 500 KB.
- SC-001: zero new Cargo dependencies.
- SC-002: zero changes to production waybill code paths.

**Scale/Scope**: 1 new corpus target. Estimated diff: ~150 lines of Rust (mostly the JS-filter helper) + 3 goldens + 1 new fork in `kusari-sandbox/` (or 1 fixture in `waybill-test-fixtures/pants_projects/` if synthetic fallback wins).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)** — ✅ PASS. Test-infra is Rust; no new dependencies of any kind.
- **Principle II (eBPF-Only Observation)** — ✅ N/A. This feature exercises waybill's filesystem-scan mode (`sbom scan --offline`), not the eBPF-trace mode. The feature adds no new discovery source; it merely locks in existing reader behavior via a regression test. The `sbom scan` filesystem-mode is an out-of-band operation model per the strict-boundary reading — this feature does not introduce lockfile-based discovery beyond what the m673 Pants pex-lockfile reader already performs.
- **Principle III (Fail Closed)** — ✅ N/A. Not a user-facing scan; a corpus test failure produces test-suite failure with a diagnostic, matching every other m195 corpus target.
- **Principle IV (Type-Driven Correctness)** — ✅ PASS. Tests are `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the existing convention; the new layer 1 assertion function returns `Result<(), AssertionFailure>` — a proper error type.
- **Principle V (Specification Compliance)** — ✅ N/A. No new SBOM fields or annotations emitted. No `waybill:*` properties introduced. This feature validates conformance, not generation.
- **Principle VI (Three-Crate Architecture)** — ✅ PASS. All changes stay within `waybill-cli`.
- **Principle VII (Test Isolation)** — ✅ PASS. Gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` per FR-007. No privileged operations. Default `cargo test` lane skips it entirely.
- **Principle VIII (Completeness)** — ✅ PASS. This feature LOCKS the current completeness posture for Pants-JS monorepos — the layer 1 assertion catches regressions that would silently drop `pkg:npm/*` components.
- **Principle IX (Accuracy)** — ✅ PASS. Assertions check for the presence of specific known components (FR-005), guarding against phantom-component-injection regressions.
- **Principle X (Transparency)** — ✅ N/A. No new emission surface.
- **Principle XI + XII (Enrichment / External Data Sources)** — ✅ N/A. No new enrichment paths.

**Gate result: PASS with no violations. No Complexity Tracking entries needed.**

## Project Structure

### Documentation (this feature)

```text
specs/675-pants-js-corpus/
├── plan.md              # This file (/speckit.plan output)
├── spec.md              # Feature spec (already exists)
├── research.md          # Phase 0 output — fixture survey + JS-filter design
├── data-model.md        # Phase 1 output — CorpusTarget + JS-filter shape
├── quickstart.md        # Phase 1 output — how to regen goldens locally
├── contracts/           # Phase 1 output — layer1 + JS-filter contracts
│   ├── layer1-assertion.md
│   └── js-golden-filter.md
├── checklists/
│   └── requirements.md  # Already generated by /speckit.specify
└── tasks.md             # Phase 2 output (/speckit.tasks — not yet generated)
```

### Source Code (repository root)

The change surface is exclusively under `waybill-cli/tests/` (production waybill code is untouched per SC-002):

```text
waybill-cli/tests/
├── corpus_harness_195/
│   ├── manifest.rs                # +1 CorpusTarget entry
│   ├── layer1_assertions.rs       # +1 assertion function
│   └── layer2_golden.rs           # +1 JS-filter pass (or new sibling module)
├── public_corpus.rs               # +1 #[test] entry
└── fixtures/
    └── public_corpus/
        └── pants-nodejs-<name>/   # NEW directory
            ├── cdx.json           # golden (JS-filtered)
            ├── spdx-2.3.json      # golden (JS-filtered)
            └── spdx-3.json        # golden (JS-filtered)
```

Plus one of the following external artifacts (mutually exclusive, resolved by research.md R1):

- **Public-monorepo path**: 1 new fork at `kusari-sandbox/<upstream-repo-name>` (matches PR #757 pattern)
- **Synthetic-fallback path**: 1 new fixture directory under `kusari-sandbox/waybill-test-fixtures/pants_projects/example-nodejs/` (matches m090 sibling-repo pattern)

**Structure Decision**: Single-project layout. All Rust changes live under `waybill-cli/tests/corpus_harness_195/` — a test-infrastructure module that already exists (m195). No new source modules; no new binaries; no new workspace members.

## Complexity Tracking

*Empty — Constitution Check passed with no gates violated.*
