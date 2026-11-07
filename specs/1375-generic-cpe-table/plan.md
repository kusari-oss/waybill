# Implementation Plan: the curated CPE table

**Branch**: `1375-generic-cpe-table` | **Date**: 2026-11-07 | **Spec**: [spec.md](./spec.md)
**Issue**: #1381

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1375**; it implements issue **#1381**.

## Summary

`pkg:generic` components are the only population a CPE helps — milestone 1373
showed `pkg:hackage` and `pkg:pypi` are already reachable through OSV, while
`pkg:generic/openssl@3.0.0` returns **zero** from OSV despite its CVEs.

`GENERIC_LIBRARY_CPES` has **12** rows. The unaddressable population is 194
components / 186 names, of which **51 have NVD dictionary entries** and none
is in the table.

**Approach**: add **48** dictionary-verified, title-confirmed rows and record
the **3** rejections in source. 48 components gain a CPE; `unaddressable`
falls 194 → 146 on the pinned closure.

Four findings shaped the plan:

1. **The vendor is almost never the name.** `bash`→`gnu`,
   `expat`→`libexpat` — the latter being the exact syft defect the probe
   script's own header cites. The sweep wildcards the vendor.
2. **Entry counts cannot decide a row**, and the obvious heuristic is worse
   than useless: "does the title mention the name" passes `ninja`, whose
   title *is* "Ninja Framework Ninja".
3. **Vendor count cannot decide one either.** `cmake` has exactly one vendor
   and it is an npm package; `kitware:cmake` returns 0. A "single vendor is
   safe" rule would have added it.
4. **The rejections must live in the source.** `ninja` and `cmake` are
   attractive to a future contributor, who will be editing that file and not
   this directory.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: none new. The table is `const` data; the enforcement test already exists (#1246).
**Storage**: N/A.
**Testing**: `./scripts/pre-pr.sh` via `scripts/gate.sh`; the existing `every_table_pair_exists_in_the_nvd_dictionary`; new tests for the vendor correctness of sampled rows and for the three rejections.
**Target Platform**: every scan emitting `pkg:generic` components.
**Performance Goals**: a linear scan of a 60-row `const` table per component; unmeasurable.
**Constraints**: no existing CPE may change (SC-005); the C209 delta must be exactly ±N (SC-004, FR-008); every row needs both a dictionary observation and a title confirmation (FR-001, FR-002).
**Scale/Scope**: 48 new rows, 60 total; 48 components affected on the pinned closure (49 carry an accepted name; `libedit`'s version is not release-shaped).

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | No dependency change; `const` data only. |
| **IV. Type-driven correctness** | ✅ PASS | No new types. The offline test makes a row without an observation a build failure rather than a silent zero. |
| **V. Specification compliance** | ✅ PASS | CPE 2.3 strings in the native `cpe`/`cpes` slots; no `waybill:*` row added. |
| **VIII. Completeness** | ✅ PASS | 48 components become matchable that nothing could reach. |
| **IX. Accuracy** | ✅ PASS | Both directions: the right vendor where NVD has one, and **no** row where its entries are a different product. |
| **X. Transparency** | ✅ PASS | Every one of the 51 dispositions recorded, including the thin call on `libxcrypt`. |
| **XII. External data source enrichment** | ✅ PASS | No scan-time lookup; the observations are dated, committed facts. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace mode, no proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS.**

## Project Structure

```text
specs/1375-generic-cpe-table/
├── spec.md                  # 8 FR, 6 SC, 3 stories, 5 edge cases
├── plan.md                  # this file
├── research.md              # R1–R7, including all 51 dispositions
├── data-model.md            # the two tables, the recorded rejections
├── contracts/table.md       # what is emitted, and what deliberately is not
├── quickstart.md            # four controls, and the one tests cannot check
├── checklists/requirements.md
├── measurements/            # the sweeps every figure rests on
│   ├── README.md
│   ├── nvd-vendor-discovery.tsv
│   ├── nvd-titles.txt
│   ├── unaddressable-names.txt
│   ├── cpe-discover.py
│   └── cpe-titles.py
└── tasks.md                 # /speckit.tasks — NOT created here
```

### Source code

```text
waybill-cli/src/generate/cpe.rs
  ├── GENERIC_LIBRARY_CPES      # 12 rows → 60
  ├── NVD_DICTIONARY_OBSERVED   # one entry per new pair
  ├── (new) a comment block recording the 3 rejections
  └── tests                     # vendor spot-checks + the rejection guard

docs/reference/ecosystems or cpe docs   # if the table is documented there
waybill-cli/tests/fixtures/golden/      # only where an affected name appears
waybill-cli/tests/fixtures/public_corpus/
```

**Structure Decision**: the existing layout. This is data plus two tests.

## Phase ordering

1. **Baseline** — the pre-change C209 value and the set of components already
   carrying a CPE, which SC-005 diffs against.
2. **US2 before US1.** The enforcement (observations, the rejection guard)
   lands before the 48 rows, so a wrong row cannot be added even momentarily.
3. **US1** — the rows themselves, in the order of `research.md` R4 so a
   reviewer can follow the dispositions.
4. **US3** — the rejections recorded in source.
5. **Goldens** — local, then corpus.

**US2 first is deliberate and inverts the priority order.** US1 is P1 because
it carries the value, but it is also the step that can introduce a silent
wrong row. Landing the guard first means the 48 rows are added into a file
that already refuses an unobserved pair.

## Risks

| Risk | Mitigation |
| --- | --- |
| **A 48-row diff is approved without reading** | quickstart Control 1 spot-checks the least-guessable vendors; Control 2 is the rejections |
| A wrong vendor ships and fails silently | `every_table_pair_exists_in_the_nvd_dictionary` (#1246) makes it a build failure. It cannot catch a *wrong product* — that is R4's judgement, with titles committed |
| `ninja` or `cmake` is added later by someone re-deriving the question | The rejections go in the source beside the table, with the titles. A test also guards them; the comment says why, the test says what |
| The C209 delta is not ±N | Control 3. Any other count moving means the change leaked out of the generic arm |
| Corpus goldens move on targets with none of these names | A task checks which goldens move rather than assuming. **A golden moving where no affected name exists is a finding** |
| `gawk`→`fossies` or `libxcrypt` judged wrongly | Both flagged in quickstart as worth a second opinion, with the evidence, so a reviewer can disagree with the specific call |
| NVD reclassifies a vendor | Already tracked as #1269; the observations are dated and the test asserts presence, not counts |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| None | — | — |

*Three shortcuts were considered and rejected, each on measured grounds.
**Deriving the vendor from the name** — `bash` would be `bash:bash` and match
nothing. **Taking the highest-entry vendor** — picks `ninjaframework` for
`ninja` and `apache` for `flex`. **Accepting any single-vendor name** — adds
`cmake`, whose one vendor is an npm package.*

---

**Phase 0 complete** — `research.md`, 7 findings including all 51 dispositions.
**Phase 1 complete** — `data-model.md`, `contracts/table.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit.tasks`.
