# Implementation Plan: Lockfile resolve graphs must be anchored to an owning component

**Branch**: `868-resolve-ownership` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/868-resolve-ownership/spec.md`

## Summary

A lockfile resolve's dependency graph is read correctly and connected to
nothing. On `lablup/backend.ai`, 760 well-formed edges sit in a document where
**1 of 331 components is reachable from the root**. The root manifest
legitimately declares nothing, so the gap is that no component owns a resolve.

The fix introduces one component per resolve the project's configuration
names, anchoring root → resolve → the resolve's top-level requirements. Per-
package resolve attribution already ships (C143) and is verified rather than
built.

**Phase 0 found that the spec conflicts with shipped behaviour** and needs an
amendment before implementation — see the gate below. It also found that the
existing name-based classifier is misclassifying two of eight resolves on the
measured target, which strengthens the spec's direction while ruling out its
literal wording.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly.
`waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `toml` (already parses `pants.toml`),
`serde`/`serde_json`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A — in-process per scan.
**Testing**: `./scripts/pre-pr.sh`; m770 quality corpus and the committed
public-corpus goldens for evidence.
**Target Platform**: Linux / macOS / Windows, unchanged.
**Project Type**: CLI + library (three-crate workspace).
**Performance Goals**: within run-to-run noise of baseline. Measured baseline
on `lablup/backend.ai` at `3ad457ae`, after warm-up: **781 / 778 / 794 ms**.
Confirmation must be **interleaved** A/B on identical machine state — a
separately-taken comparison produced a false ~3% regression in milestone 867.
**Constraints**: byte-identical output for every project with no declared
resolve (SC-004) — seventeen of eighteen corpus targets.
**Scale/Scope**: one new component kind, two new edge kinds, one additional
config key parsed, one classification precedence change, two parity-bridge
annotations.

## Constitution Check

*GATE: evaluated before Phase 0 and re-checked after Phase 1.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS — no new dependencies, no C, no linkage change. |
| **III. Fail Closed** | PASS — an undeclared resolve is not guessed at; it defaults to runtime and says so. Nothing degrades toward inventing ownership. |
| **IV. Type-Driven Correctness** | PASS — a resolve component is a distinct kind, not a package with a flag. FR-002a makes the distinction explicit rather than relying on a PURL-type convention. |
| **V. Specification Compliance** | PASS with a recorded audit. Lifecycle classification reuses the existing native-adjacent `LifecycleScope` and adds no vocabulary; resolve membership reuses C143. Two genuine bridges (resolve-nature, fallback-count) each need a catalogue row **and** extractor in the same change. |
| **VIII. Completeness** | **The principle this feature serves.** 760 real edges unreachable from the root is a false negative for any consumer that traverses, which is most of them. |
| **IX. Accuracy** | PASS, and load-bearing. Anchoring only declared resolves, and inventing no packages (A-6), keeps this from trading one false negative for a false positive. |
| **X. Transparency** | PASS — FR-003c reports classification by fallback, so the weaker signal is visible rather than silent. |

### ⚠ Gate finding: the spec needs an amendment before implementation

**FR-003a as written cannot be implemented without a regression.** It forbids
inferring classification from a resolve's name; a name-allowlist classifier
ships today (m223) and is the only signal for repositories whose tools do not
declare `install_from_resolve`.

Research R1 records the evidence and proposes declaration-first with the
heuristic retained as an explicitly-reported fallback, which contract A-4
states in its weakened form. **This is a spec change, not a plan-level
reading**, and it should be made explicitly — reinterpreting a MUST during
implementation is how a requirement stops meaning anything.

Recommended: amend FR-003a via `/speckit.clarify` (or a direct spec edit)
before `/speckit.tasks`. The plan is otherwise ready.

No other violations. No complexity deviations to justify.

## Project Structure

### Documentation (this feature)

```text
specs/868-resolve-ownership/
├── plan.md              # This file
├── spec.md              # Feature specification (clarified; FR-003a pending amendment)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── resolve-anchoring.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/pants/
├── mod.rs                  # already parses [python.resolves]; emits resolve components
├── config.rs               # + parse `install_from_resolve` back-references
└── resolve_classifier.rs   # declaration-first precedence; fallback reporting

waybill-cli/src/scan_fs/
└── mod.rs                  # anchor edges: root → resolve → top-level requirements

waybill-cli/src/generate/
└── ...                     # resolve-nature marker + fallback-count, all 3 formats

waybill-cli/src/parity/extractors/
└── mod.rs                  # EXTRACTORS entries for the two new rows

docs/reference/
└── sbom-format-mapping.md  # two new catalogue rows
```

## Phase plan

**Phase 1 — read the declaration.** Parse `install_from_resolve` and make it
authoritative over the allowlist, reporting fallback use. Output changes only
for repositories that declare it: on the measured target, `coverage-py` and
`setuptools` move to build-time. Two live misclassifications corrected before
any structural change lands.

**Phase 2 — emit resolve components.** One per declared resolve, marked as
non-package. No edges yet, so the graph is unchanged and the only delta is
component count — the cheapest point to confirm A-6 (no package invented).

**Phase 3 — anchor.** Root → resolve → top-level requirements. This is where
reachability moves, and where A-1 and A-8 are asserted against the emitted
document rather than the completeness annotation.

**Phase 4 — reporting.** Fallback-count annotation, catalogue rows plus their
extractors in the same commit.

**Phase 5 — corpus.** Re-author `pants-backend-ai`'s expectations from a CI
measurement. Its current `edges 826..1010` was authored against fabricated
fallback edges and is not a target to aim at.

Phases 1 and 2 are deliberately separable from Phase 3: if either moves a
corpus target that declares no resolve, the scoping is wrong and Phase 3 is
unsafe to land.

## Sequencing constraints

- **The FR-003a amendment precedes Phase 1.** Implementing against a
  requirement known to be wrong is how the spec becomes decorative.
- Catalogue row and extractor are one commit, never two.
- Phase 5 follows Phase 3, and measures on CI rather than locally.
- Every check observed **failing** against the pre-change build before it is
  trusted (SC-007).

## Post-Design Constitution Re-Check

Re-evaluated after Phase 1 artifacts. **No new violations**; the gate finding
above stands and is the only blocker.

Two things the design surfaced:

- Principle V's burden is **smaller than the spec assumed**: resolve
  membership (C143) and lifecycle expression (`LifecycleScope`) both already
  exist, so the feature adds two bridges rather than a vocabulary.
- Principle IX is better served than first planned. Anchoring only *declared*
  resolves means the feature cannot connect a graph on a guess — the failure
  mode is leaving something unreachable, which is visible, rather than
  asserting ownership that does not exist, which is not.

**Complexity tracking**: no entries.
