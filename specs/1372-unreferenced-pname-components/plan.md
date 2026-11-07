# Implementation Plan: the closure's build inputs that `unreferenced` hides

**Branch**: `1372-unreferenced-pname-components` | **Date**: 2026-11-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1372-unreferenced-pname-components/spec.md`
**Issue**: #1366

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1372**; it implements issue **#1366**.
> Issue **#1372** is the unrelated `image-postgres16` golden drift.

## Summary

A nix closure scan emits no component for `gzip 1.14`, `gawk 5.4.0`,
`gnused 4.9`, `less 692`, `patch 2.8` or `coreutils 9.11`. They are absent,
not merely unmatchable.

The issue was filed as a coverage gap. The measurement says otherwise:
**100 of the 101 non-emitted derivations are named by another derivation's
`inputs.drvs`** — nix's own record of what each build consumed. The one that
is not is the closure root. `unreferenced` is computed from *declared
attributes* while the build graph says something different, so the label is
not merely incomplete, it is **wrong on 741 components**.

**Approach**: build a referrer set from `inputs.drvs` inside `classify`, and
consult it **only** on the `(false, false)` arm. A declared attribute always
wins. Three lines of logic; 100 new components and 641 role corrections.

Four findings shaped the plan:

1. **The signal is already in hand.** `RawInputs.drvs` is parsed on every
   derivation and used by milestone 1280 for edges. No new parse, I/O or
   source — only a new conclusion from data the reader holds.
2. **The churn is six times the headline.** 641 already-emitting components
   are relabelled against 100 newly emitted, and FR-006 had to be narrowed
   during `/speckit-clarify` because it forbade exactly that.
3. **Emission does not become role-dependent.** The 603 milestone-1282
   members emit via `declared_pname().is_none()`, a disjunction the role
   change cannot disturb — asserted by a task rather than assumed.
4. **The golden blast radius is the inverse of milestone 1367's.** One corpus
   target changes a lot; every local golden and the other 16 targets do not.
   That is in the risk table **because** 1367's was not.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `RawInputs.drvs` (already parsed, `derivation.rs:58`), `serde`/`serde_json` (annotation values), `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. `GraphReferrers` is a per-`classify` in-memory set; nothing persists beyond the emitted document.
**Testing**: `cargo test --workspace --no-fail-fast` via `./scripts/pre-pr.sh`, plus the stub-`nix` integration pattern from `tests/closure_component_cpes.rs`, plus a synthetic fixture for the FR-003a precedence rule the real closure cannot exercise.
**Target Platform**: Any host running `waybill sbom scan --nix-closure`.
**Project Type**: Single Rust CLI workspace (three crates, Constitution VI). Changes confined to `waybill-cli`.
**Performance Goals**: One extra pass over the closure's `inputs.drvs` — 10,342 edges on the pinned closure, in-memory, no I/O.
**Constraints**: A declared attribute MUST always win (FR-003a, SC-004a). The closure root MUST stay `unreferenced` (SC-004). Member emission MUST NOT change (Edge Cases, research R4).
**Scale/Scope**: 1,275 derivations; 741 role changes; 100 newly emitted; ~45 distinct new identities.

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | Zero new dependencies; no first-party C; no new linkage. |
| **IV. Type-driven correctness** | ✅ PASS | `DerivationRole` keeps its four variants — a fifth was considered and rejected (Complexity Tracking). No `.unwrap()` in production. |
| **V. Specification compliance** | ✅ PASS | Native-field audit for C210 performed (research R8). C211 rides the existing `extra_annotations` bag. No PURL changes, so no conformance surface moves. |
| **VI. Three-crate architecture** | ✅ PASS | No new crate. |
| **VIII. Completeness** | ✅ PASS | The feature **is** this principle: 100 components present in the build and absent from the document. |
| **IX. Accuracy** | ⚠️ PASS, with the limit recorded | Admitting the 100 is accuracy. Calling all 741 `build-tooling` is a claim most of the naming attributes support and **not** each one — `allowedRequisites` is nixpkgs' assertion about the *runtime* closure. Recorded in the spec's Clarifications and research R5 rather than smoothed over. |
| **X. Transparency** | ✅ PASS | C211 per-component, C210 document-scope. A graph-admitted component is distinguishable from a declared one. |
| **XII. External data source enrichment** | ✅ PASS | No external source; `inputs.drvs` is in the closure the tier already fetched. |
| **Operating Modes** | ✅ PASS | Scan mode; generation context unchanged. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace-mode discovery, no proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS. One accuracy limit recorded rather than claimed away.**

## Project Structure

### Documentation (this feature)

```text
specs/1372-unreferenced-pname-components/
├── spec.md                      # input (16/16, 0 markers, 3 clarifications)
├── plan.md                      # this file
├── research.md                  # Phase 0 — R1..R9 + constitution alignment
├── data-model.md                # Phase 1 — GraphReferrers, the role transition, C210
├── contracts/annotations.md     # Phase 1 — new components, C182's new population, C210/C211
├── quickstart.md                # Phase 1 — why a large diff hides errors, and the four controls
├── checklists/requirements.md
├── measurements/                # created during implementation
└── tasks.md                     # /speckit-tasks — NOT created here
```

### Source code

```text
waybill-cli/src/
├── scan_fs/package_db/nix/closure/
│   ├── classify.rs              # the referrer set + the (false,false) arm
│   ├── emit.rs                  # C211 into extra_annotations; emits_as_component UNCHANGED
│   ├── derivation.rs            # UNCHANGED — inputs.drvs already parsed
│   └── graph.rs                 # UNCHANGED — more endpoints, same join
├── cli/scan_cmd.rs              # thread the C210 record
├── generate/{mod,cyclonedx,spdx}/   # C210 document scope
└── parity/extractors/           # C210 + C211 rows and six extractors

waybill-cli/tests/
├── fixtures/closure_graph_admission/   # NEW — the FR-003a precedence case
└── closure_graph_admission.rs          # NEW — stub-nix integration suite

docs/reference/sbom-format-mapping.md   # C210, C211 rows
```

**Structure Decision**: the existing layout. The change is concentrated in
`classify.rs`, which is where the role is decided; everything else is
threading a record and disclosing a signal.

## Phase ordering

1. **Foundational** — the referrer set and V1–V3, unit-testable with no scan.
2. **US2 before US1.** US2 is "`unreferenced` stops saying something untrue",
   which is the *correction*; US1 is the new components, which fall out of it.
   Doing the correction first means there is never a state where components
   are admitted on a signal the role does not reflect.
3. **US1** — the 100 newly emitting, with the six named tools asserted.
4. **US3** — C211's per-component disclosure.
5. **FR-008** — C210, its catalog rows and extractors, landing together.
6. **Measurement and goldens** — the three numbers quickstart names, then the
   single-target corpus refresh.

## Risks

| Risk | Mitigation |
| --- | --- |
| **A 641-row diff hides a wrong implementation** | quickstart Control 1 pins three exact counts measured before implementation; the closure root is the control |
| FR-003a violated — the graph overrides a declared attribute | SC-004a, and a **fixture**: the real closure cannot produce the precedence case, so the corpus would stay green |
| Member emission becomes role-dependent, regressing m1282 | Control 3 asserts the member count is identical before and after |
| Lifecycle scope changes are read as role changes | Both numbers measured separately; they will differ because scope has its own exemptions (m1276) |
| #1367's edge undershoot later diagnosed against a number this feature moved | The edge delta is recorded before and after, explicitly for that reason |
| Golden blast radius surprises the gate, as in m1367 | Predicted here: local goldens unchanged, one corpus target changes a lot. **If a local golden moves, C210/C211 is unconditional** — a bug, not a refresh |
| `allowedRequisites`-only derivations called tooling when they are runtime requisites | Recorded as an accuracy limit in the spec and research R5; not resolvable with an undifferentiated signal |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| None | — | — |

*A fifth `DerivationRole` variant (e.g. `graph-only`) was considered and
**rejected**: the role answers "artifact or tooling" and the admission signal
answers "on what evidence", so collapsing them would repeat the defect issue
#1374 records. The evidence is carried by C211 instead, which costs a catalog
row rather than a frozen enum's shape.*

---

**Phase 0 complete** — `research.md`, 9 findings, no unresolved unknowns.
**Phase 1 complete** — `data-model.md`, `contracts/annotations.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit-tasks`.
