# Implementation Plan: a binary match that merges into another reader's component stays visible on it

**Branch**: `1646-merged-binary-evidence` | **Date**: 2026-11-23 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1646-merged-binary-evidence/spec.md`. Refs #1665, #1543, #1662, #1641.

## Summary

A library whose version waybill read from its own bytes loses every trace of that when it merges into a pkg-config or CMake record. This happens to 6 of TheRock's 7 curated libraries. Separately, every binary match is labelled `manifest-analysis`, even when nothing merges.

The plan fixes both in the native evidence fields:
1. **A technique for binary analysis (R1).** A new `ResolutionTechnique::BinaryAnalysis` is set at the conversion site for banner (`embedded-version-string`) and symbol (`symbol-fingerprint`) matches. Its confidence is unchanged.
2. **Absorbed methods (R2, R3).** `ResolutionEvidence` gains `also: Vec<IdentityMethod>`. At the deduplicator's two merge sites, S1 and S2, a merge that involves a binary match adds the absorbed technique, one entry per technique.
3. **Emission (R4).** CycloneDX lists every method in `evidence.identity[].methods[]`. SPDX's D1 annotation keeps its flat keys and adds a `methods` array only when there are two or more.

The census comes first (R6). A log-only commit counts every technique change, every other binary-derived evidence kind (for the follow-up issue) and every binary merge, on TheRock and the quality corpus.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `serde`/`serde_json` (the new field and the D1 payload) and `tracing` (the census). **Zero new Cargo dependencies.**
**Storage**: N/A. In process, per scan.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in `resolve/deduplicator.rs` (`absorb_method`) and in the three emitters;
- a new `waybill-cli/tests/merged_binary_evidence.rs` (contract cases), including CycloneDX 1.6 schema validation;
- mutations M1–M6 (R8).

**Target Platform**: every host.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-007, TheRock within 1.05× `main` (median of ≥3 interleaved runs per arm).
**Constraints**:
- no new annotation, row or field in the document;
- C4, C16, C55 and C56 unchanged;
- components, PURLs, versions, CPEs and edges identical to `main` (FR-008).

**Scale/Scope**:
- `waybill-common/src/resolution.rs`: one enum variant, one struct field, one new type;
- 70 `ResolutionEvidence` literals gain `also: Vec::new()`, a mechanical edit the compiler enumerates;
- the conversion site in `scan_fs/mod.rs`;
- `resolve/deduplicator.rs`: S1, S2 and one helper;
- three emitter `match`es and the D1 payloads;
- one new test file;
- the fixture repository's `census/versions.py` and its control tree.

## Constitution Check

*GATE: must pass before Phase 0 research, and again after Phase 1 design.*

| Principle | Check | Status |
|---|---|---|
| I. Pure Rust, statically linked | No new dependency. | Pass |
| III. Fail closed | Nothing is added unless a binary match took part. Record-into-record merges are untouched (case R-1, mutation M6). | Pass |
| IV. Type-driven correctness | A typed field and a typed technique, not an internal annotation key. The technique `match`es stay exhaustive. | Pass |
| V. Specification compliance | CycloneDX's native `methods[]` and its `binary-analysis` term. SPDX uses the existing D1 row; no new `waybill:*` row. | Pass |
| VII. Test isolation | Synthetic fixtures in tempdirs. | Pass |
| VIII. Completeness | A merged component keeps the evidence of every reader that identified it. | Pass |
| IX. Accuracy | A version read from bytes is labelled as binary analysis. | Pass |
| X. Transparency | The census first, and each change counted. | Pass |

No violations, so there is no Complexity Tracking entry. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1646-merged-binary-evidence/
├── spec.md, plan.md, research.md (R1–R9), data-model.md, quickstart.md
├── contracts/wire.md
├── measurements/      # control, census, mutations, corpus, therock, gate-log
└── tasks.md           # /speckit-tasks
```

### Source Code

```text
waybill-common/src/resolution.rs            # BinaryAnalysis, IdentityMethod, ResolutionEvidence.also
waybill-cli/src/scan_fs/mod.rs              # conversion: technique from evidence_kind; census events
waybill-cli/src/resolve/deduplicator.rs     # absorb_method at S1 and S2; binary_merge census
waybill-cli/src/generate/cyclonedx/evidence.rs   # methods = primary + also
waybill-cli/src/generate/spdx/annotations.rs     # D1: flat keys + methods when 2+
waybill-cli/src/generate/spdx/v3_annotations.rs  # the same
waybill-cli/tests/merged_binary_evidence.rs      # NEW: contract cases
(70 literal sites)                          # also: Vec::new()
```

## Order of work

1. **Log-only**: the census events, with no output change. Gate, then the public corpus must pass 166 of 166.
2. **Census**: TheRock and the quality corpus (`census-versions.yml`, `--ref main`, full SHAs, `refs.txt` verified). Checkpoint: amend the spec if a count contradicts FR-002 to FR-008. File the follow-up issue for the other binary-derived kinds, with their counts.
3. **Wire**: `BinaryAnalysis` at the conversion site, then `also` and `absorb_method`, then emission.
4. **Mutations** M1–M6.
5. **Trace**: the public corpus (golden diff, regeneration, confirmation); TheRock (`measure-full` against `main`), parity and timing.
6. **Docs** (`docs/reference/sbom-format-mapping.md` D1 row, `binary-identity.md`) and the CHANGELOG.

## Complexity Tracking

None.
