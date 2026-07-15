# Data Model: Populate Remaining Public-Corpus Goldens

**Date**: 2026-07-14
**Purpose**: Enumerate the specific data-shape mutations this milestone applies. No new types are introduced — everything is a targeted update to m195 entities.

## Reused Entities (from m195 data-model)

All 5 m195 entities are reused verbatim, no changes to their shape:

- **CorpusTarget** — the manifest entry type.
- **EmittedSboms** — the harness output.
- **AssertionFailure** — Layer 1 failure diagnostic.
- **CorpusCacheKey / CorpusCacheDir** — cache location.
- **CorpusInfraError** — corpus-infra failure diagnostic.

## Mutations Applied This Milestone

### M1: postgres:16 pinned digest (FR-002)

**Location**: `mikebom-cli/tests/corpus_harness_195/manifest.rs` — the `TARGETS` const, entry `name: "image-postgres16"`, field `pinned: PinnedRef::Digest.algo_hex`.

**Before (m195 placeholder)**:

```rust
pinned: PinnedRef::Digest {
    algo_hex: "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
},
```

**After (resolved amd64 platform digest, per research §R3)**:

```rust
pinned: PinnedRef::Digest {
    // Resolved at m196 authoring time via:
    //   docker manifest inspect --verbose docker.io/library/postgres:16 \
    //     | jq -r '.[] | select(.Descriptor.platform.architecture == "amd64" \
    //         and .Descriptor.platform.os == "linux") | .Descriptor.digest'
    // This pins the linux/amd64 image specifically, so the ubuntu-latest
    // nightly runner and the maintainer's regen dispatch both pull the
    // exact same bytes.
    algo_hex: "sha256:<64-hex-resolved-at-authoring-time>",
},
```

**Validation**: `docker pull docker.io/library/postgres@<algo_hex>` succeeds against the pinned value from any Docker-authenticated environment.

### M2: Layer 1 assertion adjustments (FR-003)

**Location**: `mikebom-cli/tests/corpus_harness_195/layer1_assertions.rs` — one or more of `rust_ripgrep_layer1`, `npm_express_layer1`, `python_flask_layer1`, `maven_guice_layer1`, `image_postgres16_layer1`.

**Nature**: per-assertion, targeted. Each adjusted assertion carries a doc-comment recording (a) what m195 assumed, (b) what mikebom actually emits, (c) why the adjustment doesn't weaken the class-of-bug tripwire.

**Anti-pattern** (rejected — do NOT do this):

```rust
// BAD — silently accepts anything
if gc != "complete" && gc != "partial" && gc != "unknown" && gc != "<missing>" {
    return Err(...);
}
```

**Correct pattern** (per FR-003 acceptance scenario 1(b)):

```rust
// GOOD — narrows to specific observed reason code
if gc != "partial" {
    return Err(AssertionFailure {
        invariant_name: "graph-completeness",
        format: FailureFormat::Cdx,
        observed: gc,
        // m196: reconciled from m195 "complete" to observed "partial".
        // The observed `partial` reason is m177 TransitiveEdgesUnresolvable
        // for [generic, golang], which is the correct classifier signal
        // for a postgres image with embedded gosu binary.
        expected: "partial (m177 TransitiveEdgesUnresolvable)".to_string(),
        ...
    });
}
```

**Discovery process**: per research §R4, adjustments happen after inspecting the emitted-SBOM artifact from a first (failure-tolerant) CI regen run.

### M3: `public-corpus.yml` workflow_dispatch input (research §R1)

**Location**: `.github/workflows/public-corpus.yml` — the `workflow_dispatch` block.

**Before**:

```yaml
workflow_dispatch:
  inputs:
    branch:
      description: 'Branch to run the corpus against (defaults to main).'
      type: string
      default: main
```

**After**:

```yaml
workflow_dispatch:
  inputs:
    branch:
      description: 'Branch to run the corpus against (defaults to main).'
      type: string
      default: main
    regen_goldens:
      description: 'Regenerate public-corpus goldens in-place (writes to fixtures/, uploads as artifact).'
      type: boolean
      default: false
```

**And** in the "Run public corpus" step:

```yaml
- name: Run public corpus
  env:
    MIKEBOM_RUN_PUBLIC_CORPUS: '1'
    MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS: ${{ inputs.regen_goldens == true && '1' || '' }}
  run: |
    cargo test --test public_corpus --release -- --nocapture --test-threads=3
```

**And** a new artifact-upload step that fires when `regen_goldens == true` (so goldens land in the artifact regardless of pass/fail):

```yaml
- name: Upload regenerated goldens
  if: inputs.regen_goldens == true
  uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
  with:
    name: corpus-goldens-regen
    path: mikebom-cli/tests/fixtures/public_corpus/
    retention-days: 14
```

### M4: 15 new golden files (FR-001)

**Location**: `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json` for `target ∈ {rust-ripgrep, npm-express, python-flask, maven-guice, image-postgres16}`.

**Provenance**: written by the m195 `layer2_golden::compare_golden` helper when `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is set. Content is the mask-normalized SBOM emission from the m195 harness against the m195/m196 pinned artifacts.

**Byte-identity guarantee**: per FR-004, a second regen run on the same runner against the same pins produces byte-identical files. Verified locally after commit via a fresh CI dispatch.

## State Transitions

None. This milestone is data-mutation-only; no runtime state machine changes.
