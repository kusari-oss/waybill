# Data Model — Pants JavaScript/npm corpus regression gate

The change surface is confined to the test-infrastructure module `waybill-cli/tests/corpus_harness_195/` plus one new fixture directory. No new production data types are introduced. This document describes the concrete instantiation of existing types + the one new helper module.

## Entity 1: New `CorpusTarget` entry

**File**: `waybill-cli/tests/corpus_harness_195/manifest.rs`

**Type**: existing `CorpusTarget` struct (defined in the same file, unchanged).

**New instance**:

```rust
CorpusTarget {
    name: "pants-example-javascript",
    source: SourceKind::Git {
        clone_url: "https://github.com/kusari-sandbox/example-javascript",
    },
    pinned: PinnedRef::Sha {
        // Fork of pantsbuild/example-javascript HEAD as of 2026-09-02
        hex: "da76d5dbb407d82c136cfe8f18dc06f3c8a440e5",
    },
    ecosystem: Ecosystem::Npm,
    exercises: "npm reader stack (m066 + m147 + m180) against a Pants-managed \
                JavaScript monorepo — regression-locks issue #760 option-B behavior",
    layer1: super::layer1_assertions::pants_example_javascript_layer1,
},
```

**Placement**: Immediately after the existing `pants-example-golang` entry in the `TARGETS: &[CorpusTarget]` slice; before the trailing `];`.

**Validation rules** (inherited from existing manifest audits, no new validation):

- `public_only_audit` — passes because URL starts with `https://github.com/kusari-sandbox/` (already exempted in PR #757).
- `public_hostname_allowlist` — passes because host is `github.com`.
- `no_credentials_required` — passes when nightly CI clones the public fork (verified in R2 smoke scan).
- `cross_ecosystem_coverage_check` — this feature adds an `Npm` ecosystem entry, but that ecosystem was already covered by `npm-express` in the existing corpus, so this entry is additive rather than filling a gap. Audit passes either way.

## Entity 2: New layer 1 assertion function

**File**: `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs`

**Signature**: `pub fn pants_example_javascript_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure>`

**Assertion sequence** (fast-fail on first failure per `AssertionFailure` shape):

| # | Invariant | Observed check | Suggested action on failure |
|---|---|---|---|
| 1 | `npm-transitives-present-at-scale` | Count of `.components[].purl` matching `^pkg:npm/` ≥ 250. Observed baseline is 302 per R2. | "investigate npm reader (m066/m147/m180) or shared walker — pants-example-javascript should emit ≥ 250 pkg:npm/* components at pinned SHA" |
| 2 | `top-level-devdep-esbuild-present` | At least one component with `purl` matching `^pkg:npm/esbuild@`. | "investigate npm reader top-level-devDep resolution — package.json declares esbuild@^0.20.1" |
| 3 | `top-level-devdep-jest-present` | At least one component with `purl` matching `^pkg:npm/jest@`. | "investigate npm reader top-level-devDep resolution — package.json declares jest@^29.7.0" |
| 4 | `no-accidental-pants-annotations-on-npm` | No component matching `^pkg:npm/` carries `waybill:pants-resolve` OR `waybill:pants-target` in its `.properties[]` array. | "unexpected Pants-side provenance annotation detected on npm surface — if this is intentional (e.g., #760 option A landed), regenerate goldens and remove this assertion; otherwise investigate accidental annotation leak" |

All checks are performed against the CDX shape (`sboms.cdx`). SPDX 2.3 and SPDX 3 shapes are validated only at layer 2 (golden diff).

**Rationale**: CDX is the primary regression signal per the pattern established by every other layer 1 function in the same file. The three formats do agree via the parity extractors, so a regression that surfaces in CDX will surface in SPDX 2.3 + SPDX 3 too — but layer 2's byte-identity check would catch that. Layer 1's role is fast + human-readable diagnostic, not exhaustive coverage.

## Entity 3: New `#[test]` entry

**File**: `waybill-cli/tests/public_corpus.rs`

**New test**:

```rust
#[test]
fn corpus_pants_example_javascript() {
    run_target("pants-example-javascript");
}
```

**Placement**: Immediately after the existing `corpus_pants_example_golang` test.

## Entity 4: New JS-filter helper module

**File**: `waybill-cli/tests/corpus_harness_195/js_filter.rs` (new file)

**Public API** (per R3 decision):

```rust
/// Structurally filter a CDX 1.6 JSON document to the `pkg:npm/*` surface only.
/// Retains: `.metadata`, `.bomFormat`, `.specVersion`, `.serialNumber`, `.version`,
/// plus `.components[]` entries whose `.purl` starts with `pkg:npm/`, plus
/// `.dependencies[]` entries where `.ref` is a kept component AND `.dependsOn` is
/// filtered to only edges pointing at kept components.
pub fn filter_cdx_to_js(v: &mut serde_json::Value);

/// Same, for SPDX 2.3 JSON. Filters `.packages[]` by any `externalRefs[]` PURL
/// starting with `pkg:npm/`. Filters `.relationships[]` where both endpoints
/// reference kept SPDXIDs. Retains `.creationInfo`, `.documentDescribes`, envelope.
pub fn filter_spdx23_to_js(v: &mut serde_json::Value);

/// Same, for SPDX 3.0.1 JSON-LD. Filters `@graph[]` component nodes whose
/// associated PURL starts with `pkg:npm/`. Filters relationship nodes where both
/// `from`/`to` reference kept elements. Retains doc-scope nodes (`CreationInfo`,
/// `SpdxDocument`).
pub fn filter_spdx3_to_js(v: &mut serde_json::Value);
```

**Behavior notes**:

- Filters MUST be idempotent: applying twice yields the same output.
- Filters MUST NOT modify the source `serde_json::Value` reference in-place if invoked on an already-filtered value (idempotent).
- Filters MUST preserve JSON ordering of retained elements — the existing byte-identity comparison depends on stable serialization ordering.

## Entity 5: Golden fixture directory

**Path**: `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/`

**Files**:

| File | Format | Regen source |
|---|---|---|
| `cdx.json` | CycloneDX 1.6 JSON | `waybill sbom scan --format cyclonedx-json` → mask non-determinism → `filter_cdx_to_js` |
| `spdx-2.3.json` | SPDX 2.3 JSON | `waybill sbom scan --format spdx-2.3-json` → mask → `filter_spdx23_to_js` |
| `spdx-3.json` | SPDX 3.0.1 JSON-LD | `waybill sbom scan --format spdx-3-json` → mask → `filter_spdx3_to_js` |

**Constraint**: Combined size ≤ 500 KB (SC-004). Empirical measurement post-filter is a Phase-2 task deliverable — if any format exceeds its share of 500 KB, revisit R3's filter design.

**Regen mode**: `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` + `WAYBILL_RUN_PUBLIC_CORPUS=1` — existing m195 pattern, unchanged.

## Layer 2 compare-golden flow (updated)

**File**: `waybill-cli/tests/corpus_harness_195/layer2_golden.rs::compare_golden`

**New behavior**: After the existing `mask_nondeterministic()` pass and BEFORE `serde_json::to_vec_pretty()`, dispatch on target name:

```rust
if target == "pants-example-javascript" {
    match format {
        FailureFormat::Cdx => js_filter::filter_cdx_to_js(&mut masked),
        FailureFormat::Spdx23 => js_filter::filter_spdx23_to_js(&mut masked),
        FailureFormat::Spdx3 => js_filter::filter_spdx3_to_js(&mut masked),
        FailureFormat::All => unreachable!("layer 2 is per-format"),
    }
}
```

**Byte-identity guarantee**: For every existing target (`go-cobra`, `rust-ripgrep`, ..., `pants-example-golang`), the layer 2 flow is byte-identical to pre-feature. Only `pants-example-javascript` invokes the new filter.

## Data volume and scale

- **New files committed**: 4 (3 goldens + 1 filter module) + existing manifest/assertions/test-entry files touched.
- **New fork repository**: 1 (`kusari-sandbox/example-javascript`, ~95 KB).
- **Corpus cache footprint at nightly runtime**: +95 KB per run (cached in `~/.cache/waybill/corpus/<source-id>/<sha>/`).
- **Nightly CI runtime increment**: <60s per SC-005 (95 KB clone + fast scan + fast layer 1 + small golden compare).

## No state transitions

The corpus target is stateless per test invocation. Each `run_target` call scans a fresh temp dir. The corpus cache is opportunistic; a cache-miss triggers a re-clone.
