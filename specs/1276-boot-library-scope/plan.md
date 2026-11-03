# Implementation Plan: a package the compiler ships is never scoped as absent from the artifact

**Branch**: `1276-boot-library-scope` | **Date**: 2026-11-03 | **Spec**: [spec.md](./spec.md)
**Input**: `/specs/1276-boot-library-scope/spec.md` | **Research**: [research.md](./research.md)

## Summary

A GHC boot library can carry `optional` or `excluded` because its only path
through the dependency graph runs via a non-runtime component — although the
compiler ships it in every build. Measured: 10 of 78 boot-library components
across the corpus, in two targets, and 1 on `slack-web`.

Read against the vendored CycloneDX 1.6 schema this is not the semantics
question #1340 took it for. `optional` is defined as "not capable of being
called due to them **not being installed**", with the explicit instruction
that an installed-but-prohibited component "**must be scoped as `required`**";
`excluded` as "not reachable within a call graph at runtime". A boot library is
installed and is in the call graph, so both values are wrong by the schema's
own text. The feature is a **conformance fix**, not a preference.

**Phase 0 changed the shape of the work.** The fix is not one site. The
project already has an invariant demanding the marker this feature removes —
**I3**, "a component reachable from the subject only through non-runtime edges
carries a non-runtime marker" — and it is clean on every corpus target today.
Measured: **all 10** affected components become I3 violations once unmarked,
I3 has **zero** declarations, and an undeclared violation fails the lane. So
I3's amendment is part of this feature and lands in the same change.

Two further Phase 0 findings shrink the work: the Q1 clarification
(FR-004a, narrow to the flake's named series) is already satisfied by
`candidate_series` and needs a test rather than code, and FR-006's carrier is
C170, which already records the fact the exemption rests on.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (the
document-scope value), `std::collections::{BTreeSet, HashMap, HashSet}`,
`tracing`. The nulled-name set is already parsed by
`haskell_packages/boot_libraries.rs` (m926) and the propagation's immunity
concept already exists in `resolve/scope_propagation.rs` (m1209/m1269).
**Zero new Cargo dependencies.** No network, no subprocess, no new filesystem
read (FR-013).
**Storage**: N/A — in-process per scan. The per-revision cache at
`~/.cache/waybill/nixpkgs/<rev>/` is read as it already is.
**Testing**: `cargo test`; unit tests on the immunity predicate; the corpus
invariant harness for I3's amendment and FR-002a's new invariant; milestone
1275's `nix_closure_hackage_identity` suite for `t057`'s inversion. Mutation-
tested per requirement.
**Target Platform**: any host; no test needs `nix` or the network.
**Project Type**: CLI (single Rust workspace).
**Performance Goals**: no measurable change, and unlike milestone 1275 that
claim is cheap to make good: the exempt set is already computed during
enrichment and is handed over rather than re-derived (R5), so the feature adds
one set membership test per component in the propagation's existing loop.
**Constraints**: FR-003 — no component outside the exempt class changes scope;
measured control is 569 of the corpus's 579 non-runtime scopes surviving.
FR-012 — existing wire values keep their meaning.
**Scale/Scope**: 78 boot-library components corpus-wide, of which 10 are
mis-scoped; 2 targets affected; 1 invariant amended, 1 added, 1 document-scope
row added, 1 existing test inverted.

## Constitution Check

*GATE: passed before Phase 0, re-checked after Phase 1.*

| principle | assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No new dependency, so the linkage posture is unchanged. |
| **II. eBPF-Only Observation** | Not engaged. User-space only. |
| **III. Fail Closed** | Scoped to the eBPF trace path. For enrichment, Principle XI governs: FR-005 makes an unreadable compiler configuration exempt nothing rather than exempt everything, which is the fail-closed direction for this feature — the dangerous default is treating a component as immune without evidence. |
| **IV. Type-Driven Correctness** | The exempt set is a `BTreeSet<String>` of nulled names, which is what m926 already produces; the immunity decision is a predicate over it rather than an `Option` plus a comment. No `unwrap` outside `#[cfg(test)]`. |
| **V. Specification Compliance** | **This principle is the feature.** The CycloneDX 1.6 `meta:enum` definitions quoted in the spec's Context make the current output non-conformant, and Principle V's first clause requires conformance "to the SBOM envelope and to every sub-element within it". Separately, its standards-native clause applies to FR-007's carrier: the audit is in research R4, the result is one new document-scope row (KEEP-NO-NATIVE, parity-bridge), and **the spec must gain that citation before implementation** — see Risks. FR-006 needs no new field; C170 already carries the fact. |
| **VI. Three-Crate Architecture** | The change lives in `waybill-cli`. `waybill-common` gains nothing. |
| **VII. Test Isolation** | Every test seeds a `tempfile::tempdir()` and points `WAYBILL_NIXPKGS_CACHE` at it; no test reads the host's cache. Fixtures use `waybill-fixture-*` names only (#285, #640). The `slack-web` and corpus-golden figures come from measurement probes, which the project's convention allows to use real coordinates. |
| **VIII. Completeness** | Engaged in the direction of *not* under-reporting: 10 components presently filtered out of "what is in the artifact" by a scope the schema says is wrong for them. |
| **IX. Accuracy** | The governing principle alongside V. The risk this feature introduces is the mirror image of the one it fixes — over-exempting, which would report a genuinely test-only component as runtime. FR-003's control and the measured 569-of-579 figure exist for that, and FR-004a's narrowing bounds it. |
| **X. Transparency** | FR-006 (per-component, via C170) and FR-007 (document-scope count, including zero). The count exists so a changed scope reads as a rule change rather than a project change. |
| **XI. Enrichment** | Degrade and disclose: FR-005 — an unreadable compiler configuration exempts nothing and the scan still succeeds. |
| **XII. External Data Source Enrichment** | No new external source. Same pinned revision, same cache, same files m926 already reads. |

**No violations.** One obligation carried forward rather than a violation:
FR-007's audit result must be cited in the spec before implementation, which
is Principle V's explicit requirement and the first task in Phase 2.

## Project Structure

### Documentation (this feature)

```text
specs/1276-boot-library-scope/
├── spec.md                  # 16 FRs, 10 SCs, 3 stories, 2 clarifications
├── plan.md                  # this file
├── research.md              # Phase 0 — R1-R8, all measured
├── data-model.md            # Phase 1
├── quickstart.md            # Phase 1
├── contracts/README.md      # Phase 1
├── checklists/requirements.md
├── measurements/
│   └── scope-always-present.py   # the corpus probe, committed
└── tasks.md                 # /speckit.tasks output — not created here
```

### Source code

```text
waybill-cli/src/resolve/scope_propagation.rs
    # the immunity predicate gains the exempt class. The concept already
    # exists — `runtime_reachable` — so this extends it rather than
    # introducing one. PropagationSummary gains the exemption count.

waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs
    # + `EnrichmentSummary.boot_libraries`, so the set `enrich` already
    #   computes at line 941 reaches the propagation's call site (R5).
    #   The same shape m1275 used for `package_set`.

waybill-cli/src/cli/scan_cmd.rs
    # thread the set from the summary into the propagate call at 4851.

waybill-cli/src/generate/ + docs/reference/sbom-format-mapping.md
    # FR-007's document-scope row, with its Principle V audit clause and
    # its three parity extractors. A catalogue row without an extractor
    # fails `every_catalog_row_has_an_extractor`, so they land together.

waybill-cli/tests/corpus_harness_195/invariants/
├── checks/scope_propagation.rs   # I3 AMENDED — exempt the same class
├── checks/<new>.rs               # FR-002a's invariant, I3's complement
└── mod.rs                        # the new invariant's registry entry

waybill-cli/tests/nix_closure_hackage_identity.rs
    # m1275's `t057` inverted: it currently asserts the defect (FR-010)
```

## Phases

**Phase 0 — research.** Complete. Eight findings, every number measured. One
changed the shape of the work (R1, I3), two shrank it (R2, R3).

**Phase 1 — design.** Complete: `data-model.md`, `contracts/README.md`,
`quickstart.md`.

**Phase 2 — tasks.** `/speckit.tasks`. Expected shape: the spec amendment for
FR-007's audit citation comes first because the constitution requires it
before implementation; then US1 (the immunity, I3's amendment and `t057`'s
inversion as one indivisible change, because the corpus lane is red between
them); then US2 (per-component reason, already carried by C170 — a test plus
the verification R3 asks for); then US3 (the document-scope count and its row).

## Risks

**I3 and the fix must land together, and the window between them is a red
lane.** Measured: all 10 affected components become undeclared I3 violations
the moment the marker is removed, and an undeclared violation fails the lane
rather than reporting a cell. There is no ordering in which the tree is green
with one half applied, so they are one task rather than two — and the
temptation to declare the 10 in `invariant-declarations.toml` to get a green
intermediate must be refused: that file records defects awaiting a fix, and
these would be components the feature deliberately made correct.

**The mirror-image defect is the one to watch.** This feature's plausible
wrong implementation over-exempts, reporting a genuinely test-only component
as runtime — the same error the feature fixes, pointing the other way. Three
things bound it: FR-003's requirement, the measured control (569 of 579
non-runtime scopes must survive), and FR-004a's narrowing, whose cost is known
(the union exempts up to 15 names a given series does not).

**FR-007's carrier needs a spec amendment before implementation.** Principle V
requires the audit cited in the spec's Functional Requirements. Milestone 1275
recorded the cost of treating that as a formality: its plan asserted C151 was
reusable, the catalogue forbade it, and the correction arrived three commits
later. The audit is done (R4); writing it into the spec is the first task.

**The exempt class and C170 are expected to coincide, and that is a
measurement.** R3 argues they must, because both derive from the same nulled
set through `classify()`. A component in the boot set that does not carry C170
would be exempt without recording why, which is FR-006 unsatisfied. C5 asserts
the coincidence rather than trusting the argument.

**`t057`'s inversion changes a test written five commits ago to assert the
opposite.** That is correct and is why FR-010 forbids deleting it — the
fixture is the only deterministic reproduction of a boot library inheriting a
scope through closure-derived edges. The risk is inverting the assertion while
dropping the premise checks that keep it from passing vacuously; R7 names
them.

**This branch is stacked on milestone 1275.** If 1275 lands first the stack
collapses naturally. If it does not, this feature's PR carries 1275's seven
commits, and the golden refresh 1275 needs is a prerequisite for reading this
feature's own corpus diff.
