# Implementation Plan: The same build gives the same answer, scan after scan

**Branch**: `1542-deterministic-binary-merge` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)

## Summary

#1600. On the built ROCm SDK, one component's reachability varied from scan to scan of one build. The FR-001 trace reproduced it synthetically, flipping in 4 of 12 scans (`measurements/trace.md`), and found two causes:

1. **The deduplicator merges different binaries that share a file name.** Its group key omits the `file-sha256` qualifier, so the SDK's two `libgotcha.so`, the ends of symlink chains to different libraries, become one component. The loser's identifications keep a parent that no longer exists, and are emitted detached. The spec's premise ("same hash") is corrected.
2. **Spec 1541's containment edges depend on component order.** A rule reads an adjacency map that another rule writes in the same loop, and the deduplicator's output order varies between processes, so reachability varies over a fixed document.

**Fixes:**
- **R1:** add `file-sha256` to the dedup key.
- **R2:** build containment from a snapshot of the dependency edges.
- **R3:** make the deduplicator's survivor and output order deterministic.

Each of the first two, alone, was confirmed by the reproduction. Then audit: repeated scans of every corpus target and the SDK (R5).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.

**Primary Dependencies**: Existing only: `resolve::deduplicator`, `generate::graph_completeness`, `serde_json`. **Zero new Cargo dependencies** (FR-008).

**Storage**: N/A.

**Testing**:
- **Unit:**
  - the dedup key, the survivor and the output order;
  - `add_containment_edges` with components in every order of a small set (exhaustive permutations).
- **Integration:** `waybill-cli/tests/scan_determinism.rs` with cases D-1 to D-6. Each fixture is scanned at least 20 times and compared masked. I-A is asserted with the corpus harness's own functions.
- **Mutations:**
  - each rule removed;
  - the snapshot reverted to the live map;
  - the qualifier dropped from the key;
  - the tie-breaks removed.
- **CI:**
  - the public corpus with `repeat=2`, at the branch and at `main`;
  - `measure-full` keeping every scan's documents;
  - the quality corpus against `main`.

**Target Platform**: as today. **Project Type**: CLI.

**Performance Goals**: SC-007, at most 1.1× `main`.

**Constraints**: deterministic; no network; synthetic fixtures; TheRock in CI only.

**Scale/Scope**: the SDK has 2,749 components (2,105 top-level and 644 nested).

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust | Pass. No new crates. |
| III. Fail Closed | Pass. Two different files are never reported as one. An identification is never re-parented onto a file it did not come from (R1, alternatives rejected). |
| IV. Type-Driven | Pass. The key gains a field. No new stringly paths beyond the existing `file-sha256` parse (scan_fs/mod.rs:995 pattern). |
| V. Spec Compliance | Pass. No wire change. CycloneDX nesting, SPDX `CONTAINS` and the bom-ref shape are unchanged. |
| VII. Test Isolation | Pass. Synthetic fixtures. TheRock documents probed from CI artifacts. |
| VIII/IX/X | Pass. A repeatable document is the premise of every comparison waybill and its consumers make. |

No violations.

## Project Structure

```text
specs/1542-deterministic-binary-merge/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md                      # D-1…D-6, I-A, I-D
├── measurements/trace.md, probe_repro.rs  # FR-001
└── tasks.md
waybill-cli/src/resolve/deduplicator.rs                 # R1 key, R3 survivor and order
waybill-cli/src/generate/graph_completeness/mod.rs      # R2 snapshot
waybill-cli/tests/scan_determinism.rs                   # D-1…D-6
waybill-cli/tests/corpus_harness_195/…                  # repeat-and-compare (R5)
.github/workflows/public-corpus.yml                     # `repeat` input
(fixture repo) .github/workflows/measure-full.yml       # keep every scan's documents
```

## Complexity Tracking

None.
