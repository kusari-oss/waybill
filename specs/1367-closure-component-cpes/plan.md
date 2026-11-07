# Implementation Plan: a CPE for the closure components a feed could match

**Branch**: `1367-closure-component-cpes` | **Date**: 2026-11-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1367-closure-component-cpes/spec.md`
**Issue**: #1371

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1367**; it implements issue **#1371**.
> Issue **#1367** is the unrelated closure edge undershoot.

## Summary

A `--nix-closure` scan emits zero CPEs. Six components match milestone 097's
curated table exactly by name, carry clean release versions, and receive
nothing — because the synthesis pass runs over the component set before the
closure's components are added to it.

**Approach**: add a second, **fill-only-if-empty** synthesis pass in
`scan_cmd::execute`, after the closure merge and after the milestone-1366
identity pass, guarded by a release-shape version test. Do not move the
existing pass. Add one document-scope record so "no mapped software" and "the
pass did not run" stop being the same document.

Four findings shaped the plan, all read from the tree:

1. **Moving the pass would break an unrelated consumer.** `package_db::read_all`
   has two production callers — `scan_fs/mod.rs:457` and `report/mod.rs:77`.
   A move strips CPEs from the report path.
2. **Unconditional assignment would destroy operator data.**
   `supplement::merge` lands `--supplement-cdx` CPEs at `scan_cmd.rs:4334`,
   *before* any post-closure pass. Fill-if-empty makes FR-004 and FR-005 true
   by construction rather than by care.
3. **FR-003 is not satisfied today.** `synthesize_cpes` gates on
   `version.is_empty()` and nothing else; `format_cpe` escapes, so a strange
   version yields a well-formed CPE that matches nothing. The guard is new
   work, and belongs at the new call site rather than inside the shared
   function.
4. **The delivered count will exceed six, and that is consistency rather than
   scope creep** — the 74 `pkg:pypi` components from milestone 1366 hold empty
   `cpes` and will take the `"pypi"` arm. Labelled a prediction; a task
   measures it.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `generate::cpe::synthesize_cpes` and its curated table (unchanged), `serde`/`serde_json` (the C209 value), `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. The pass is a pure in-memory transform; nothing persists beyond the emitted document.
**Testing**: `cargo test --workspace --no-fail-fast` via `./scripts/pre-pr.sh`, plus a stub-`nix` integration suite on the milestone-1366 fixture pattern, plus a synthetic fixture for the `--supplement-cdx` precedence case the pinned closure cannot demonstrate.
**Target Platform**: Any host running `waybill sbom scan`. The new pass is not closure-specific.
**Project Type**: Single Rust CLI workspace (three crates, Constitution VI). Changes confined to `waybill-cli`.
**Performance Goals**: One additional pass over the component set, calling a pure function. ~942 components on the measured closure; no I/O, no network, no subprocess.
**Constraints**: Components that already carry CPEs MUST be untouched (FR-004, FR-005). Closure members MUST NOT receive CPEs (FR-002). Non-release versions MUST NOT (FR-003).
**Scale/Scope**: 6 measured targets from the curated table; 942 components scanned; 539 members and 117 unmapped names deliberately unchanged.

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | Zero new dependencies; no first-party C; no new linkage. |
| **IV. Type-driven correctness** | ⚠️ PASS with a noted deviation | `cpes` stays `Vec<String>`. A `Cpe` newtype is the principle's preference for a domain value, and introducing one would touch five emitters and every golden for a refactor this feature does not need. Recorded in Complexity Tracking rather than passed silently. |
| **V. Specification compliance** | ✅ PASS | CPE 2.3 formatted-string binding is already implemented and escaped (`cpe.rs:319-344`). Native-field audit for C209 performed; the per-component carrier is the existing **C19**, so no new per-component row. |
| **VI. Three-crate architecture** | ✅ PASS | No new crate. |
| **VIII. Completeness** | ✅ PASS | Closes a false negative: software present in the build, invisible to an NVD-backed scanner. |
| **IX. Accuracy** | ✅ PASS | The governing principle. CPEs are emitted as **candidates**; FR-002 and FR-003 keep them off things that are not software or cannot match. US2 is this principle restated. |
| **X. Transparency** | ✅ PASS | FR-008's C209 makes "no mapped software" distinguishable from "the pass did not run" — the confusion that let this defect live since milestone 097. |
| **XII. External data source enrichment** | ✅ PASS | No external source. In-source table, closure-recorded version. |
| **Operating Modes** | ✅ PASS | Scan mode; generation context unchanged. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace-mode discovery, no proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS. One deviation recorded below rather than waved through.**

## Project Structure

### Documentation (this feature)

```text
specs/1367-closure-component-cpes/
├── spec.md                      # input (16/16, 0 markers)
├── plan.md                      # this file
├── research.md                  # Phase 0 — R1..R9 + constitution alignment
├── data-model.md                # Phase 1 — the release-shape rule, C209, state transitions
├── contracts/annotations.md     # Phase 1 — candidates, C209, CLI, compatibility
├── quickstart.md                # Phase 1 — the vacuous-pass trap and three controls
├── checklists/requirements.md
├── measurements/                # created during implementation
└── tasks.md                     # /speckit-tasks — NOT created here
```

### Source code

```text
waybill-cli/src/
├── generate/cpe.rs              # UNCHANGED — the synthesizer and its table
├── cli/scan_cmd.rs              # the new fill-if-empty pass + the release-shape guard
├── generate/mod.rs              # C209 on ScanArtifacts
├── generate/cyclonedx/          # C209 document property
├── generate/spdx/               # C209 document annotation ×2
└── parity/extractors/           # C209 row + three extractors

waybill-cli/tests/
├── fixtures/cpe_supplement/     # NEW — the --supplement-cdx precedence case
└── closure_component_cpes.rs    # NEW — stub-nix integration suite

docs/reference/sbom-format-mapping.md   # C209 row
```

**Structure Decision**: the existing layout. The new pass lives in `scan_cmd`
beside the other post-merge passes because that is the only place the full
component set exists; `generate/cpe.rs` is deliberately untouched so the
synthesizer's behaviour for every existing ecosystem is byte-stable.

## Phase ordering

1. **Foundational** — the release-shape predicate and its unit tests. No
   wiring; testable offline.
2. **US2 before US1.** US2 is "things that are not software get no CPE", which
   is satisfied by *declining*. Implementing the guards first means the pass is
   never, even briefly, in a state where it emits for members.
3. **US1** — the fill-if-empty pass at the call site, with the six-component
   integration test and its control.
4. **US3** — the `--supplement-cdx` precedence case and its synthetic fixture.
5. **FR-008** — C209, its catalog row and its three extractors, landing
   together.
6. **Measurement and goldens** — the measured total (not the predicted one),
   then the corpus refresh with every differing target accounted for.

## Risks

| Risk | Mitigation |
| --- | --- |
| An unconditional fill silently overwrites `--supplement-cdx` CPEs | Fill-if-empty; a synthetic fixture test that goes red under mutation 2 in `quickstart.md` |
| The six-component test passes vacuously on an empty population | A control asserting the fixture contains mappable software, written before the CPE assertion |
| More components change than expected, and the golden diff is read as a surprise | R7 states the mechanism and labels the total a prediction; a task measures it before the refresh |
| `image-postgres16`'s unrelated drift (#1372) is swept into this feature's goldens | The golden diff is accounted for file by file; that target is excluded explicitly |
| FR-003's guard is untestable on the corpus | Stated in `data-model.md`: all declining shapes exist on the closure but none is mapped, so the test needs a synthetic fixture |
| A `Cpe` newtype is expected by review | Recorded in Complexity Tracking with the reason, not left for a reviewer to raise |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| `cpes` remains `Vec<String>` rather than a `Cpe` newtype (Principle IV prefers the newtype for a domain value) | The field is pre-existing and read by five emitters and the supplement merge path. This feature populates it; it does not introduce it. | Introducing the newtype here would touch `waybill-common`, three emitters, the supplement parser and every golden carrying a CPE — a refactor with its own blast radius, bundled into an ordering fix. The constitution's own guidance against speculative refactors bundled with fixes applies. Worth its own issue. |

---

**Phase 0 complete** — `research.md`, 9 findings, no unresolved unknowns.
**Phase 1 complete** — `data-model.md`, `contracts/annotations.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit-tasks`.
