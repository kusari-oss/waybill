# Contract — pants_example_javascript_layer1

**Module**: `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs`

## Signature

```rust
pub fn pants_example_javascript_layer1(
    sboms: &EmittedSboms,
) -> Result<(), AssertionFailure>;
```

- **Input**: `sboms` — the emitted CDX + SPDX 2.3 + SPDX 3 Values produced by the corpus harness against the pinned SHA of `pantsbuild/example-javascript`.
- **Output**: `Ok(())` on all-invariants-hold; `Err(AssertionFailure)` on first failure with a class-of-bug-oriented diagnostic.
- **Determinism**: MUST be deterministic — same `sboms` in → same result out. No RNG, no timestamps, no environment reads.

## Invariants and diagnostic contract

Each invariant reports via `AssertionFailure { invariant_name, format, observed, expected, suggested_action }`. All checks are performed against `sboms.cdx` — SPDX 2.3 + SPDX 3 are validated by layer 2 (golden diff) only.

### Invariant 1 — `npm-transitives-present-at-scale`

- **Check**: count of `sboms.cdx.components[]` where `.purl` matches `^pkg:npm/` is `>= 250`.
- **Observed baseline (per research R2)**: 302 at pinned SHA.
- **Threshold selection**: 250 is tighter than SC-006's 10% floor (which would allow drops to 271). Chosen to catch smaller regressions per-target than the spec-level minimum.
- **On failure**:
  - `invariant_name = "npm-transitives-present-at-scale"`
  - `format = FailureFormat::Cdx`
  - `observed = format!("{count} pkg:npm/* components")`
  - `expected = "at least 250 pkg:npm/* components (observed baseline 302 at pinned SHA)"`
  - `suggested_action = "investigate npm reader (m066 / m147 / m180) or shared walker — pants-example-javascript at pinned SHA should emit ≥ 250 pkg:npm/* components"`

### Invariant 2 — `top-level-devdep-esbuild-present`

- **Check**: at least one `sboms.cdx.components[]` where `.purl` starts with `pkg:npm/esbuild@`.
- **Rationale**: `package.json.devDependencies` declares `esbuild@^0.20.1`; this is a top-level direct devDep and MUST resolve.
- **On failure**:
  - `invariant_name = "top-level-devdep-esbuild-present"`
  - `format = FailureFormat::Cdx`
  - `observed = "no pkg:npm/esbuild@* component"`
  - `expected = "at least one pkg:npm/esbuild@X.Y.Z component (top-level devDep declared in package.json)"`
  - `suggested_action = "investigate npm reader top-level-devDep resolution — package.json declares esbuild@^0.20.1"`

### Invariant 3 — `top-level-devdep-jest-present`

- **Check**: at least one `sboms.cdx.components[]` where `.purl` starts with `pkg:npm/jest@`.
- **Rationale**: `package.json.devDependencies` declares `jest@^29.7.0`; dual-anchor with invariant 2 catches regressions that break exactly one direct-dep resolution.
- **On failure**:
  - `invariant_name = "top-level-devdep-jest-present"`
  - `format = FailureFormat::Cdx`
  - `observed = "no pkg:npm/jest@* component"`
  - `expected = "at least one pkg:npm/jest@X.Y.Z component (top-level devDep declared in package.json)"`
  - `suggested_action = "investigate npm reader top-level-devDep resolution — package.json declares jest@^29.7.0"`

### Invariant 4 — `no-accidental-pants-annotations-on-npm`

- **Check**: no `sboms.cdx.components[]` where `.purl` starts with `pkg:npm/` carries either `waybill:pants-resolve` OR `waybill:pants-target` in its `.properties[]` array (per property name field).
- **Rationale**: FR-006 documents that Pants-side provenance annotations are EXPECTED absent on npm components today (issue #760 option A is the tracked follow-up if this changes). This invariant enforces the regression-lock.
- **On failure**:
  - `invariant_name = "no-accidental-pants-annotations-on-npm"`
  - `format = FailureFormat::Cdx`
  - `observed = format!("{count} pkg:npm/* components carry unexpected Pants annotations")` (include a bounded sample of offending PURLs, cap the list length to keep the diagnostic scannable)
  - `expected = "no pkg:npm/* component carries waybill:pants-resolve or waybill:pants-target (FR-006 regression-lock)"`
  - `suggested_action = "unexpected Pants-side provenance annotation on npm surface. If intentional (e.g., #760 option A landed), regenerate goldens + remove this assertion + update spec 675 FR-006. If unintentional, investigate annotation leak."`

## Execution order

Invariants MUST be checked in order 1 → 4. First failure returns immediately. All checks share the same input `sboms` reference; no mutation.

## Test coverage of the assertion itself

The assertion function is exercised by `corpus_pants_example_javascript` in `public_corpus.rs` (gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1`). No standalone unit test is required — the m195 corpus harness pattern has never had one, and the assertion's semantics are covered by the goldens (any bug in the assertion would either false-fail on a good scan, caught during regen, or false-pass a regression, caught in later nightly runs).

## What this contract does NOT cover

- **Byte-identity of the full SBOM** — that's layer 2's job (`compare_golden`).
- **Absence of specific npm packages** — negative assertions of the form "no `pkg:npm/some-suspicious-package` exists" are out of scope. If a specific package becomes a signal, extend the contract.
- **Dependency edge correctness** — a regression in the dependency graph without component-count change is caught by layer 2, not layer 1.
