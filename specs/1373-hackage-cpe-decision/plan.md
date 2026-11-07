# Implementation Plan: what `eligible_unmapped` is actually counting

**Branch**: `1373-hackage-cpe-decision` | **Date**: 2026-11-07 | **Spec**: [spec.md](./spec.md)
**Issue**: #1375

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1373**; it resolves issue **#1375**.

## Summary

`waybill:cpe-coverage` (C209) reads `eligible_unmapped: 284` on the pinned
closure. Issue #1375 asked whether that is a gap or a metric overstating one.

Measurement says **both, in different places, and the second is worse**. The
count mixes 163 `pkg:hackage` components that OSV already addresses with
`pkg:generic` components that no feed can address — and its release-shape gate
excludes most of the latter, so the number of genuinely unreachable components
is **733**, not 284. A reader acting on 284 was under-informed by a factor of
2.6, in the direction that matters.

**Approach**: replace `eligible_unmapped` with two counts that, together with
`with_candidates`, partition every component — `addressable_without_cpe` and
`unaddressable`. No CPE arm is added, and no component's identifiers change.

Five findings shaped the plan:

1. **`pkg:generic` is addressable by nothing**, measured: `openssl@3.0.0` and
   `zlib@1.2.11` return zero from OSV despite well-known CVEs. A CPE is their
   only route to a feed, which is what makes them the real gap.
2. **A non-zero probe is not sufficient to call a type addressable.** This
   spec's own first rule said it was, and that rule admits `generic` on one
   OSS-Fuzz record. Both halves — OSV's declared list *and* a probe — are
   required (research R5).
3. **C209 is not closure-gated**, contradicting four statements in the first
   draft. 33 local goldens and 48 corpus goldens carry it, so the blast radius
   is the **inverse of milestone 1372**. (The spec said 66 until
   implementation measured it: half the `grep` hits are `*.actual.json`
   diagnostics rather than tracked goldens.)
4. **NVD has 1 of 163 Haskell names**, and the three with live advisories —
   `base`, `Cabal`, `process` — have no dictionary entry at all. No arm.
5. **The 24 versionless `pkg:hackage` components are in no count today.**
   Partitioning every component rather than the eligible subset is what fixes
   that, and it is also what makes the sum checkable.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (the row's payload), `tracing`. **Zero new Cargo dependencies.** No scan-time network access is added; the ecosystem list is a committed, dated observation.
**Storage**: N/A. The classification is computed once per scan and emitted.
**Testing**: `./scripts/pre-pr.sh` via `scripts/gate.sh`, plus unit tests on the classifier, plus an independent recompute of the row from the emitted document (`measurements/partition.py`).
**Target Platform**: every `waybill sbom scan` invocation — the row is not closure-gated.
**Project Type**: single Rust CLI workspace (Constitution VI). Changes confined to `waybill-cli`.
**Performance Goals**: one extra pass over the component slice; no I/O.
**Constraints**: no component's `cpes` may change (FR-004, SC-004); `with_candidates` keeps its meaning (FR-006); the counts must partition the component set (FR-002).
**Scale/Scope**: 977 components on the pinned closure; 81 tracked golden files carry the row (33 local, 48 corpus).

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | No new dependency; probes are measurement-time and ship nothing. |
| **IV. Type-driven correctness** | ✅ PASS | A closed `FeedAddressability` enum with an exhaustive match, not an allowlist that falls out of date (#892 survived ~2 years behind one). No `.unwrap()` in production. |
| **V. Specification compliance** | ✅ PASS | Native-field audit performed: no CDX or SPDX construct expresses "which feed can address this component". C209 already exists; its payload changes, carriers do not. |
| **VIII. Completeness** | ✅ PASS | The row describes the document more completely than it did. |
| **IX. Accuracy** | ✅ PASS | The feature **is** this principle. The current row is wrong in both directions, and the understatement is the one that costs an operator something. |
| **X. Transparency** | ✅ PASS | The counts become verifiable against the component count, which they are not today. |
| **XII. External data source enrichment** | ✅ PASS | No scan-time lookup. The ecosystem list is committed and dated, and the plan states it is an observation rather than a truth. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace mode, no proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS. No violations, no justifications needed.**

## Project Structure

```text
specs/1373-hackage-cpe-decision/
├── spec.md                      # 8 FR, 6 SC, 2 clarifications, 4 corrections
├── plan.md                      # this file
├── research.md                  # R1..R9 + constitution alignment
├── data-model.md                # FeedAddressability, the table, the v2 row
├── contracts/annotations.md     # C209 v1→v2, blast radius, measured expectations
├── quickstart.md                # four controls, in catching order
├── measurements/                # probes + committed external observations
│   ├── README.md
│   ├── osv-ecosystems-2026-11-07.txt
│   ├── nvd-sweep.tsv / nvd-retry.tsv
│   ├── osv-probe.py / osv-purl-probe.py / osv-purl-direct.py
│   ├── name-gap-probe.py
│   └── partition.py             # SC-001's independent recompute
└── tasks.md                     # /speckit.tasks — NOT created here
```

### Source code

```text
waybill-cli/src/
├── cli/scan_cmd.rs              # CpeSynthesisCoverage -> v2; the classifier
├── generate/cpe.rs              # FR-007: record why there is no hackage arm.
│                                #   Otherwise UNCHANGED — no arm is added.
└── parity/extractors/           # expected UNCHANGED; a task verifies it

waybill-cli/tests/fixtures/golden/        # 33 files regenerate
waybill-cli/tests/fixtures/public_corpus/ # 48 files, 16 of 17 targets
docs/reference/sbom-format-mapping.md     # C209's row text
```

**Structure Decision**: the existing layout. The change is concentrated in one
struct and one classification function.

## Phase ordering

1. **Measure first.** The pre-change figures for every affected target are
   already captured in `contracts/annotations.md`; the remaining baseline is
   the per-component `cpes` snapshot that SC-004 compares against.
2. **The classifier**, unit-testable with constructed components and no scan.
3. **US1** — the row's new shape and the sum invariant.
4. **US2** — the per-ecosystem composition is already measurable from the
   document; the task is to confirm the row and the document agree.
5. **US3 / FR-007** — record the no-arm decision in `generate/cpe.rs`.
6. **Goldens.** Local first (33 files), because a local golden moving in an
   unexpected *target* is the signal that the change leaked. Corpus second.

**US2 before US1 was considered and rejected**: US1 defines the counts US2
reports on, so the reverse order would mean writing a breakdown of fields that
do not exist.

## Risks

| Risk | Mitigation |
| --- | --- |
| **An 81-file golden diff hides a wrong implementation** | quickstart Control 2 recomputes the row from the document independently; Control 1 is the sum |
| The change leaks beyond the metric into component output | Control 3 diffs every `{purl, cpe, cpes}`; SC-004 |
| A new PURL type is silently counted `unaddressable` | closed enum + exhaustive match, not an allowlist (`feedback_enum_allowlist_falls_out_of_date`) |
| A type is called addressable on list membership alone, or on a probe alone | FR-003 requires both; `generic`'s OSS-Fuzz hit is the worked example in research R5 |
| `image-postgres16` appears as an unexplained 17th changed target | It carries no C209 at all — verified against the m1372 regen artifact — and stays excluded for #1372 |
| The blast radius surprises the gate, as it did in m1367 | Predicted here in files and targets: 33 local, 48 corpus, 16 of 17 targets. **If a local golden does *not* move, the row is not being emitted where it was** |
| OSV's ecosystem list changes and the table silently rots | The table is dated, committed, and the plan says it is an observation; re-probing is the documented way to add a row |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| None | — | — |

*Keeping `eligible_unmapped` alongside the new fields was considered and
**rejected**: it is wrong in both directions, so emitting it would mean
shipping a number this feature exists because it misleads. The `/v1` → `/v2`
schema bump is what tells a consumer the shape changed, rather than letting an
absent key read as zero.*

---

**Phase 0 complete** — `research.md`, 9 findings, no unresolved unknowns.
**Phase 1 complete** — `data-model.md`, `contracts/annotations.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit.tasks`.
