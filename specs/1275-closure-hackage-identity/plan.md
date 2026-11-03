# Implementation Plan: closure members that are known Hackage packages

**Branch**: `1275-closure-hackage-identity` | **Date**: 2026-11-03 | **Spec**: [spec.md](./spec.md)
**Input**: `/specs/1275-closure-hackage-identity/spec.md` | **Research**: [research.md](./research.md)

## Summary

`--nix-closure` appends 276 `pkg:generic` components with no license, hash or
edges, while the same scan already resolves 65 `pkg:hackage` components with
47 licenses, 47 hashes and 245 out-edges — from the same cached file, in the
same process. The fix gives the closure-appended members the identity the
pinned package set already holds for them.

**The approach is constrained by what Phase 0 found rather than chosen freely.**
The root cause is ordering: the Haskell enrichment runs at `scan_cmd.rs:4540`
and the closure merge at `4729`. The obvious fix — move the merge earlier —
was tried in milestone 1035 and **measured to downgrade 41 PURLs** from
`pkg:hackage` to `pkg:generic`, and `is_enrichable` only accepts versionless
components so the existing pass would skip closure members anyway. So: a new
pass *after* the merge, reading two things already in memory.

Identity rests on **two independent signals** — an exact `(name, version)`
match against the default attribute, and the derivation having been built by a
GHC — because 26 of 276 components have a Hackage name and nothing to do with
Haskell, and a name-only match would mint `pkg:hackage/bash@5.3p9` for GNU
bash.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation
values), `regex` (already parses this file), `tracing`, `anyhow`. The
`PackageSetEntry` fields the feature reads (`version`, `source_hash`,
`license`, `runtime_relations`) are all already parsed, by m926, m1273 and
m985 respectively. **Zero new Cargo dependencies.** No subprocess, no network,
no new filesystem read.
**Storage**: N/A — in-process per scan. The per-revision package-set cache at
`~/.cache/waybill/nixpkgs/<rev>/` is read as it already is; the pass adds no
cache and no persistence.
**Testing**: `cargo test`; a new integration suite driving the closure tier
through the stub `nix` of `nix_config_closure.rs`, with a seeded package-set
cache per `nix_haskell_resolution_m926.rs`. Mutation-tested per requirement
via `measurements/mutate.py`.
**Target Platform**: any host with `nix`; the suite's stub-driven tests need
neither `nix` nor network.
**Project Type**: CLI (single Rust workspace).
**Performance Goals**: no measurable change. The pass is one lookup per
closure component against a `HashMap` already built — 276 lookups on the
measured fixture. No new I/O, so there is nothing for it to be slow at.
**Constraints**: FR-011 — the closure's membership, derivation count and role
classification must be byte-identical. FR-010 — no new surface. #980 — a PURL
rewrite must move its edges in the same step, because the alternative
validates.
**Scale/Scope**: measured at 276 closure components of which 59 reclassify,
278 declared edges of which 265 resolve and 13 need a boot-library target.
#1282 measured the same order on a real library: 103 of 316.

## Constitution Check

*GATE: passed before Phase 0, re-checked after Phase 1.*

| principle | assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No new dependency at all, so the linkage posture is unchanged. |
| **II. eBPF-Only Observation** | Not engaged. User-space only; `waybill-ebpf` untouched. |
| **III. Fail Closed** | Scoped by its own text to the eBPF trace path. For enrichment, Principle XI governs — see below. |
| **IV. Type-Driven Correctness** | `HackageIdentity` is an exhaustive enum with a variant per outcome, not `Option<Purl>` plus a comment. FR-007's reason is then a value rather than a reconstruction, and a later variant cannot silently lose its reason the way #1218 and #892 record. No `unwrap` outside `#[cfg(test)]`. |
| **V. Specification Compliance** | **Standards-native first, and it applies here.** The feature emits no new `waybill:` property for identity, license, hash or edges: `purl`, `licenses`, `hashes` and `dependsOn` are all native CycloneDX/SPDX carriers. Only FR-007's reason and FR-013's tier marker need an annotation, and both reuse existing rows (`waybill:unresolved-reason`, the existing tier marker) rather than adding one. |
| **VI. Three-Crate Architecture** | The pass lives in `waybill-cli`, beside the tier it serves. `waybill-common` gains nothing. |
| **VII. Test Isolation** | Mandatory and called out in `quickstart.md`: every test seeds a `tempfile::tempdir()` and points `WAYBILL_NIXPKGS_CACHE` at it. **No test reads the host's `~/.cache/waybill/nixpkgs/`.** Fixtures use `waybill-fixture-*` names only (#285, #640). |
| **VIII. Completeness** | This is the principle the feature serves. 276 components currently matchable by nothing become 59 matchable ones plus 217 that say why they are not. |
| **IX. Accuracy** | The governing principle, and the reason for the two-signal rule. A `pkg:hackage` PURL is a claim about identity; 26 measured collisions show a name match does not support it. A hash that does not decode to 32 bytes yields no hash rather than a malformed one, as m926 already does. |
| **X. Transparency** | FR-007: every unreclassified member names which of five outcomes it was. FR-009: an unavailable package set is recorded, not silent. |
| **XI. Enrichment** | Degrade and disclose, never fail. FR-009 is this principle: the scan succeeds, the members stay generic, and the document says the pass did not run. |
| **XII. External Data Source Enrichment** | No new external source. The pinned revision is the same source m926 established, read from the same cache. |

**No violations, and no complexity to justify.** The feature adds one pass and
one enum; the data, the parser, the cache and the edge-to-relationship
conversion all already exist.

## Project Structure

### Documentation (this feature)

```text
specs/1275-closure-hackage-identity/
├── spec.md                  # 18 FRs, 11 SCs, 3 stories, 0 markers
├── plan.md                  # this file
├── research.md              # Phase 0 — R1-R9, all measured
├── data-model.md            # Phase 1
├── quickstart.md            # Phase 1
├── contracts/README.md      # C1-C21
├── checklists/requirements.md
├── measurements/
│   └── size-closure-members.py   # the sizing probe, committed
└── tasks.md                 # /speckit.tasks output — not created here
```

### Source code

```text
waybill-cli/src/scan_fs/package_db/nix/
├── closure/
│   ├── emit.rs              # unchanged — see research R1
│   └── mod.rs               # unchanged
└── haskell_packages/
    ├── package_set.rs       # unchanged — every field already parsed
    ├── boot_libraries.rs    # reused for FR-012c's targets
    ├── closure_identity.rs  # NEW — the pass, the enum, the two signals
    └── mod.rs               # + `mod closure_identity;`

waybill-cli/src/cli/scan_cmd.rs
    # one call site, AFTER merge_closure_components (4729), before emission

waybill-cli/tests/
├── nix_closure_hackage_identity.rs   # NEW — US1, US2, US3
├── nix_config_closure.rs             # must stay 14/14 (C17)
└── fixtures/nix_closure_hackage/     # NEW — seeded package set, synthetic names
```

## Phases

**Phase 0 — research.** Complete. Nine findings, every number measured. Two
changed the design: R1 (the obvious reorder is already measured wrong) and R6
(edges bind by name, and the 13 dangling targets are boot libraries).

**Phase 1 — design.** Complete: `data-model.md`, `contracts/README.md` (C1-C21),
`quickstart.md`.

**Phase 2 — tasks.** `/speckit.tasks`. Expected shape: US1 (identity, license,
hash) is the MVP; US2 (collision safety) is what makes US1 publishable rather
than dangerous and should land in the same PR; US3 (reasons) and FR-012
(edges) are separable.

## Risks

**The two-signal rule could be over-strict.** Measured 60 of 60 agreement
today, so it costs nothing now — but a nixpkgs change that stops a Haskell
derivation referencing a GHC store path would silently stop reclassifying.
FR-006 requires the agreement be measured rather than asserted, and the
committed probe reports it, so the failure is visible as a drop in the
reclassified count rather than as silence.

**The alias gap is accepted, with its cost stated.** `hashable 1.4.7.0`
matches only `hashable_1_4_7_0`, so the default-attribute rule misses 1 of 60.
A reverse `pname → attributes` index would catch it and would introduce a new
ambiguity when two aliases share a version. Deferred, measured, recorded in
research R4 — not discovered later.

**A PURL rewrite that loses its edges validates.** #980's failure mode is
silent in all three formats. This is the one place where a correct-looking
document is the symptom, which is why `data-model.md` §4 makes the rename map
part of reclassification rather than a following step, and why C15 is asserted
directly rather than inferred from an edge count.

**Edge count will look worse than sbomnix, on purpose.** sbomnix emits 8,230
by treating store references as dependencies. This feature emits only declared
ones. If the count is ever the measure, the honest graph loses — stated in the
spec's Assumptions so it is a known trade rather than a surprise.
