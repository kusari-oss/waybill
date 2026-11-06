# Implementation Plan: components for the closure members waybill does not name

**Branch**: `1282-closure-nameless-components` | **Date**: 2026-11-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification at `specs/1282-closure-nameless-components/spec.md`

## Summary

65% of the pinned closure — 1,145 of 1,749 derivations — carries no `pname`,
and the tier emits nothing for any of it. This milestone emits all of them,
each carrying one of four derived roles, with an identity that is unique even
under a qualifier-ignoring comparison. Sources and packages join the
dependency graph as stopping points; scaffolding and fragments are emitted but
never become edge endpoints, and carry a reason saying why they are orphans.

The component count for this tier goes from 381 to roughly 1,479, and the
edge set is reshaped rather than merely grown — measured −35% in a
derivation-level proxy, because stopping earlier replaces transitive fan-outs
with direct edges.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.

**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation
values), `regex` (the version-shaped-name test in the classifier; already a
direct dep), `waybill_common::types::purl::{Purl, encode_purl_segment}`,
`tracing`. **Zero new Cargo dependencies.** No subprocess, no network, no
filesystem access: every input is a closure already parsed into memory.

**Storage**: N/A — in-process per scan, as with every closure milestone since
1035.

**Testing**: `./scripts/pre-pr.sh` with no `+toolchain` (#1189), plus a new
integration suite driving the real binary against a stub `nix`, following the
milestone-1280 and milestone-1281 fixture pattern. Corpus goldens refresh only
through `public-corpus.yml` dispatch.

**Target Platform**: wherever the closure tier already runs.

**Project Type**: single Rust workspace.

**Performance Goals**: none specified, and deliberately (research R7). The work
is proportional to a closure already in memory; the one superlinear risk is
milestone 1280's projection, which this feature widens but does not
restructure.

**Constraints**: identity must survive a qualifier-ignoring comparison
(FR-002); the milestone-1277 census must not move (FR-012); no edge may name
an absent component (FR-013); a scan without `--nix-closure` must stay
byte-identical (FR-010).

**Scale/Scope**: 1,145 new components on the measured closure — 549 `source`,
83 `package`, 267 `scaffolding`, 246 `fragment` — merging to ~1,098 after the
content-hash dedup.

## Constitution Check

| Principle | Gate | Verdict |
| --- | --- | --- |
| I. Pure Rust, statically linked | No new crates, no C, no subprocess | **PASS** |
| III. Fail closed | Nothing here can fail: the data is already parsed. A derivation that cannot be classified still emits, under the role its evidence supports | **PASS** |
| IV. Type-driven correctness | The role is an enum, not a string; the identity basis is a two-variant choice, not an `Option` a caller must remember to check | **PASS** |
| V. Specification compliance | Components use each format's native component construct. The role is a `waybill:` annotation because no format has a native carrier for "this is a build scaffold" | **PASS** |
| IX. Accuracy | **The gate that deserves argument.** See below | **PASS, with the argument recorded** |
| X. Transparency | FR-009 counts every role, FR-014 explains every orphan, FR-018 carries the new edge figure. A reader can reconcile the component count rather than inferring it | **PASS** |
| Boundary 1 (no lockfile discovery in trace mode) | Scan-mode only | **PASS** |

### Principle IX, argued rather than ticked

Principle IX says waybill MUST minimise false positives — "components listed in
the SBOM that were not actually used by the traced build". This feature adds
513 components that are build scaffolding and bootstrap fragments, including
`strncpy`. A reviewer is entitled to ask whether that is exactly what the
principle forbids.

It is not, and the distinction is the one the principle itself draws. A false
positive is a component **that was not there**. Every one of these was there:
nix recorded it in the closure of the thing that was built, and each is
identified by a hash nix itself computed. Emitting them is not a guess that
turned out wrong; it is a fact a consumer may not want.

What the principle protects against is a document whose signal is diluted by
things a consumer cannot distinguish from real dependencies. FR-005 and FR-007
are that protection: every added component states which of four kinds it is,
and a consumer can filter to sources and packages and obtain exactly the 585
that are. The scope was chosen by the operator on that basis, and User Story 2
exists to hold the condition.

The honest residual risk is a consumer who filters nothing and reads `strncpy`
as a dependency. That is a real cost, accepted deliberately, and recorded in
the spec's Clarifications rather than discovered later.

## Project Structure

### Documentation (this feature)

```text
specs/1282-closure-nameless-components/
├── spec.md
├── plan.md                    # this file
├── research.md                # Phase 0 — R1..R7
├── data-model.md              # Phase 1
├── quickstart.md              # Phase 1
├── contracts/
│   └── member-classification.md
├── checklists/requirements.md
└── measurements/
    ├── classify-nameless.py   # the probe; re-runnable on any closure JSON
    ├── results.txt
    └── README.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── member.rs       # NEW — the four-role classifier and the identity basis
├── emit.rs         # emit every member; identity from the role; role annotation
├── graph.rs        # `stops` widened to source|package
├── summary.rs      # the FR-009 role record and the FR-018 edge figure
└── mod.rs          # unchanged beyond wiring

waybill-cli/src/generate/
└── orphan_reason.rs  # one new variant for the FR-014 population

waybill-cli/tests/
└── closure_members.rs  # NEW — integration suite over emitted documents
```

**Structure Decision**: single project. The classifier is a new module beside
the existing `classify.rs` rather than inside it, because the two answer
different questions over the same closure — `classify.rs` assigns the C182
role to **derivations**, `member.rs` assigns the new role to the things that
become **components** — and research R4 showed how easily those two are
confused. Separate files make the distinction visible at the import site.

## Complexity Tracking

No constitutional violations. The one gate needing argument (Principle IX) is
argued above rather than waived.

## What this plan deliberately does not settle

The **hash-prefix length** for the identifier. Research R6 establishes that
full hashes are collision-free over this population; a truncated prefix is not
the same claim. The task that chooses a length must re-run the probe over the
prefix and assert zero collisions, rather than adopt a conventional length and
assume it holds.
