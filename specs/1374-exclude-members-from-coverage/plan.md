# Implementation Plan: `unaddressable` counts patches and tarballs as software

**Branch**: `1374-exclude-members-from-coverage` | **Date**: 2026-11-07 | **Spec**: [spec.md](./spec.md)
**Issue**: #1380

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1374**; it fixes issue **#1380**. Issue
> **#1374** is the unrelated member-role question.

## Summary

Milestone 1373's `unaddressable` reads **733** on the pinned closure. **539
are closure members** — source tarballs, patches, build fragments. The honest
figure is **194**.

The synthesizer already refuses members and says why: a member *is not
installed software*. The counting loop does not, so a thing declined on those
grounds is then counted as software no feed can address — the conflation C209
exists to prevent, one population over from the one it was built for.

**Approach**: two new variants on the existing closed enum, two new counts,
and the computation moved to the one point where both excluded populations
exist. The row goes to `/v3` with five counts that sum to the document's own
component count.

Five findings shaped the plan:

1. **Nothing in the suite could have caught this.** The file-tier version of
   the same error was caught because it made the row disagree with the
   document; the member version makes nothing disagree, because both sides
   count members.
2. **File-tier is a second excluded population and it is large** — 117 on
   `image-postgres16`, present on 11 of 17 targets. Counting members but not
   files would leave the sum checkable on only 6 of 17.
3. **The counts must move, not just change.** File-tier components join the
   vector at `scan_cmd.rs:5315`, 408 lines after the counts are computed.
4. **`with_candidates` is structural; `addressable_without_cpe` is not.** No
   member carries a CPE on either host. One member on linux carries an
   OSV-addressable PURL, so that count does move — which **falsified FR-004**
   as written.
5. **The two hosts differ by 2×.** Linux carries 1,087 members to darwin's
   539, so the spec's headline figures must not be quoted against the corpus.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (the row's payload), `tracing`. **Zero new Cargo dependencies.** No new I/O, no network.
**Storage**: N/A. The counts are computed once per scan and emitted.
**Testing**: `./scripts/pre-pr.sh` via `scripts/gate.sh`; unit tests on the classifier; an independent five-count recompute from the emitted document (`measurements/partition.py`, already proven able to disagree).
**Target Platform**: every `waybill sbom scan` — C209 is not closure-gated.
**Performance Goals**: one pass over the component vector; no I/O.
**Constraints**: `with_candidates` must not change (FR-004); no identifier may change (FR-006); the five counts must partition the emitted component set (FR-002); the counts must be computed where both excluded populations exist (FR-009).
**Scale/Scope**: 977 components darwin / 1,615 linux; 81 tracked golden files carry the row.

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | No dependency change. |
| **IV. Type-driven correctness** | ✅ PASS | Two new variants on the closed `FeedAddressability` enum, exhaustively matched — **not** a filter ahead of it, which would leave the enum describing a population it no longer covers and break the guarantee that every component was considered (#892). No `.unwrap()` in production. |
| **V. Specification compliance** | ✅ PASS | No native CDX/SPDX construct records which components a feed can address. C209 exists; only its payload changes. |
| **VIII. Completeness** | ✅ PASS | Both excluded populations are counted rather than vanishing. |
| **IX. Accuracy** | ✅ PASS | The feature **is** this principle: the row calls a patch file unaddressable software. |
| **X. Transparency** | ✅ PASS | The sum becomes checkable against the raw document, which under `/v2` it was not. |
| **XII. External enrichment** | ✅ PASS | No scan-time lookup; the ecosystem list is unchanged. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace mode, no proxy, no C, no `.unwrap()`, no file-tier behaviour change (only its count is read). |

**Result: PASS. No violations, no justifications needed.**

## Project Structure

```text
specs/1374-exclude-members-from-coverage/
├── spec.md                      # 9 FR, 6 SC, 2 clarifications, 1 correction
├── plan.md                      # this file
├── research.md                  # R1..R8 + constitution alignment
├── data-model.md                # the two variants, the five counts, the move
├── contracts/annotations.md     # /v2 → /v3, per-target expectations, blast radius
├── quickstart.md                # four controls, and why the golden diff is weakest
├── checklists/requirements.md   # 16/16, plus the cross-host risk
├── measurements/partition.py    # Control 2 — already proven able to DISAGREE
└── tasks.md                     # /speckit.tasks — NOT created here
```

### Source code

```text
waybill-cli/src/cli/scan_cmd.rs
  ├── FeedAddressability          # + ExcludedMember, + ExcludedFile
  ├── classify_addressability     # exclusion precedence, membership first
  ├── CpeSynthesisCoverage        # + two counts, wire() → /v3, total()
  └── execute                     # counting moves from 4914 to after 5315

docs/reference/sbom-format-mapping.md     # C209's row text
waybill-cli/tests/fixtures/golden/        # 33 files regenerate
waybill-cli/tests/fixtures/public_corpus/ # 48 files, 16 of 17 targets
```

**Structure Decision**: the existing layout. The change is one enum, one
function, and moving one computation.

## Phase ordering

1. **Baseline.** Capture the pre-change document and the per-component
   identifier snapshot SC-004 diffs against, before anything moves.
2. **The classifier** — two variants and their precedence, unit-testable with
   constructed components and no scan.
3. **US1** — the counts, the `/v3` bump, and the move past line 5315.
4. **US2** — the sum property, asserted and independently recomputed.
5. **Goldens** — local first, then the corpus.

**The move (step 3) must land with the counts, not before.** Moving the
computation while it still produces three fields would emit `/v2` numbers
computed over a vector that now includes file-tier components, which is a
state worse than either endpoint: the sum would match nothing.

## Risks

| Risk | Mitigation |
| --- | --- |
| **An 81-file diff of identical one-line changes is approved without reading** | quickstart Control 2 recomputes from the components themselves, so it fails when the row is internally consistent but wrong about the document. The diff is explicitly named the weakest signal |
| Moving the computation changes `with_candidates` | It cannot: synthesis stays at 4907 and only counting moves, so the CPE-bearing set is identical at both points. Asserted by Control 4 and SC-003 |
| A member re-enters the partition through `with_candidates` | FR-003 excludes by membership before any other test, so it holds even if the synthesizer's guard changed |
| Darwin figures quoted against the linux corpus | Both are in `contracts/annotations.md` as separate tables; linux carries 2× the members. This is the error milestone 1366 recorded once |
| `addressable_without_cpe` moving is read as a defect | It moves by exactly 1 on `nix-closure-moat` (`pkg:hackage/time`), stated in the contract. Any other movement **is** a finding |
| The golden count is mis-taken as 66 | 33 of the 66 `grep` hits are `*.actual.json` diagnostics; milestone 1373 recorded this after a complete regeneration looked half-applied |
| A third excluded population appears later | The enum's exhaustive match plus Control 2's sum would surface it as an unclosed sum rather than a silent miscount |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| None | — | — |

*Two simpler shapes were considered and rejected. **A single `excluded`
field** sums identically and collapses "is a source tarball" into "is a file"
— this milestone's own conflation, one level down. **Filtering before
classification** instead of adding variants is fewer lines, but leaves the
closed enum describing a population it no longer covers, so the exhaustive
match stops proving every component was considered.*

---

**Phase 0 complete** — `research.md`, 8 findings, no unresolved unknowns.
**Phase 1 complete** — `data-model.md`, `contracts/annotations.md`, `quickstart.md`, `measurements/partition.py`, agent context updated.
**Next**: `/speckit.tasks`.
