# Implementation Plan: an ecosystem identity for the closure components a feed could match

**Branch**: `1366-closure-ecosystem-purl` | **Date**: 2026-11-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1366-closure-ecosystem-purl/spec.md`
**Issue**: #1365

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1366**; it implements issue **#1365**.
> Issue #1366 is an unrelated feature.

## Summary

Of the 942 components a nix closure scan emits, 781 carry `pkg:generic` and so
no vulnerability feed can match them. 539 of those are closure members for
which generic is correct; this milestone addresses the **83 Python derivations
(75 distinct names)**. This milestone
gives the confirmable ones a `pkg:pypi` identifier and declines the rest.

**Approach**: between the closure merge and the closure edge build, walk the
appended components, derive PyPI candidates from the closure's own derivation
records, confirm each `(name, version)` against the existing deps.dev
resolution path, and rewrite the identifier **only** on a confirmed candidate.
Everything else keeps today's identifier byte-for-byte.

Three findings shaped the plan, all read from the tree rather than assumed:

1. **The pass has exactly one correct slot** (research R11). Edges carry PURL
   *strings*, and `closure::graph::relationships` reads `c.purl` live at
   `scan_cmd.rs:4833`. Running after the merge (4748) and before that call
   means corrected identifiers flow into the edges for free; running after it
   orphans every component the feature fixes.
2. **The confirmation states already exist.** `LookupResult` is
   `Found | Absent | Failed | Unqueried` and the client already treats a 404 as
   `Ok(None)` — *"an answer … `Err` is reserved for failures that said nothing
   about the package"*. FR-003's hardest clause is a property of code that is
   already merged.
3. **The corpus cannot witness the feature** (research R7). Its one closure
   target runs `--no-deps-dev`, so it will always take the degraded path. The
   test strategy is built around that rather than discovering it later.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation values), `reqwest` via the existing `DepsDevClient`, `tracing`, `anyhow`. Reuses `pip::normalize_pypi_name_for_purl` (`pip/mod.rs:99`), `EnrichmentKey` (`request_key.rs:41`), `DepsDevSource` + its disk cache, `waybill_common::types::purl::Purl`. Dev: existing `wiremock = "0.6"` (`waybill-cli/Cargo.toml:236`). **Zero new Cargo dependencies.**
**Storage**: N/A for new state. Confirmation shares the existing deps.dev disk cache at `~/.cache/waybill/deps-dev/` (FR-011); nothing else persists.
**Testing**: `cargo test --workspace --no-fail-fast` via `./scripts/pre-pr.sh`, plus a wiremock-stubbed US1 test (the only place confirmation actually fires) and a recorded manual measurement for SC-001.
**Target Platform**: Any host running `waybill sbom scan --nix-closure`. Measurements are aarch64-darwin on pinned nixpkgs `a799d3e3`; corpus goldens pin x86_64-linux — a *different closure*.
**Project Type**: Single Rust CLI workspace (three crates, Constitution Principle VI). Changes are confined to `waybill-cli`.
**Performance Goals**: ≤ 75 additional deps.dev lookups per closure scan, cache-backed, batched. Measurably zero network cost on a warm cache and on every `--no-deps-dev` run.
**Constraints**: Confirmation MUST NOT be able to fail a scan (FR-012). The 539 closure members MUST be byte-identical (FR-005, SC-003). Other readers' components MUST be byte-identical (FR-006, SC-004).
**Scale/Scope**: 83 candidate derivations / 75 distinct names on the pinned closure, out of 1,275 derivations and 942 emitted components.

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — both passes identical, no design change moved a verdict.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | Zero new Cargo dependencies; no first-party C; no new dynamic linkage. |
| **IV. Type-driven correctness** | ✅ PASS | Identifiers flow as `Purl`, never `String`. No `.unwrap()` in production (the `clippy::unwrap_used` deny at the crate root enforces it, inside `#[cfg(test)]` too). |
| **V. Specification compliance** | ✅ PASS | Native-field audit performed against the vendored schemas at `waybill-cli/tests/fixtures/schemas/` (research R5), **not from memory**. CDX 1.6 `component.evidence.identity[]` is a native carrier for "an identifier was concluded by this technique" and **is adopted**; SPDX 2.3 / 3.0.1 have none, so C207 is a documented parity bridge. Every emitted PURL goes through `Purl::new`, so an invalid one cannot reach output. |
| **VI. Three-crate architecture** | ✅ PASS | No new crate. |
| **VIII. Completeness** | ✅ PASS | Component count is invariant; the feature changes identity, not membership. |
| **IX. Accuracy** | ✅ PASS | This principle *is* US2: *"ambiguous or low-confidence matches MUST be flagged rather than silently included as definitive."* An unconfirmed candidate keeps today's identifier. |
| **X. Transparency** | ✅ PASS | FR-007 per-component basis, FR-008 document-scope counts, FR-009 unavailability recorded. CDX uses the spec-native `evidence` mechanism, which Principle X prefers over ad-hoc extensions. |
| **XII. External data source enrichment** | ✅ PASS | Scan mode. C1 *no new components* — holds by construction, the pass mutates an existing set. C2 *provenance annotated* — FR-007. C3 *unavailability must not prevent generation* — FR-012. C4 *scanned evidence is the authority* — the closure names the package; deps.dev only confirms it exists. |
| **Operating Modes** | ✅ PASS | Scan mode throughout; `waybill:generation-context` unchanged. |
| **Strict Boundary 1** | ✅ N/A | Trace mode only. |
| **Strict Boundaries 2–5** | ✅ N/A | No proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS, both gates. Complexity Tracking is empty — there is nothing to justify.**

### One judgement worth recording

`--no-deps-dev` suppresses confirmation, which means the only corpus target
that exercises this tier can never confirm anything (research R7). The
alternative — querying deps.dev despite the operator's flag — was rejected:
a flag that does not mean what it says is a worse defect than a blind spot that
is written down and compensated for. The compensation is Control 1 and
Control 2 in `quickstart.md`, and SC-007 exists to keep it honest.

## Project Structure

### Documentation (this feature)

```text
specs/1366-closure-ecosystem-purl/
├── spec.md                      # input (16/16 checklist, 0 markers)
├── plan.md                      # this file
├── research.md                  # Phase 0 — R1..R11 + constitution alignment
├── data-model.md                # Phase 1 — 3 entities, validation rules V1..V5
├── contracts/
│   └── annotations.md           # Phase 1 — PURL change, C207, C208, CLI, compatibility
├── quickstart.md                # Phase 1 — the corpus trap and the three controls
├── checklists/requirements.md   # from /speckit.specify
├── measurements/                # SC-001's recorded run (created during implementation)
└── tasks.md                     # /speckit.tasks — NOT created here
```

### Source code

```text
waybill-cli/src/
├── scan_fs/package_db/nix/closure/
│   ├── ecosystem_identity.rs      # NEW — candidate rule, outcomes, the pass
│   ├── derivation.rs              # unchanged (reuses attr_str's two carriers)
│   ├── emit.rs                    # unchanged (FR-005 — members keep pkg:generic)
│   └── graph.rs                   # unchanged (reads c.purl live; see research R11)
├── enrich/
│   └── depsdev_source.rs          # narrow pub(crate) entry onto fetch_results
├── cli/
│   └── scan_cmd.rs                # the pass invoked in the R11 window (~4760–4833)
├── generate/cyclonedx/
│   └── evidence.rs                # C207 CDX-native: an evidence.identity[] entry
├── generate/spdx/                 # C207 parity bridge + C208 document scope
└── parity/extractors/
    ├── mod.rs                     # C207, C208 EXTRACTORS rows
    ├── cdx.rs  spdx2.rs  spdx3.rs # the six extractor fns

waybill-cli/tests/
├── fixtures/nix_closure_ecosystem/closure.json   # NEW — synthetic, waybill-fixture-*
└── closure_ecosystem_identity.rs                 # NEW — wiremock-stubbed US1/US2/US3

docs/reference/sbom-format-mapping.md             # C207, C208 rows
```

**Structure Decision**: the existing three-crate layout, unchanged. The new
module sits beside the closure reader's other passes because it reads closure
evidence; it is *invoked* from `scan_cmd` rather than from the reader, because
the reader runs before the merge and has no access to `DepsDevSource`
(research R1).

## Phase ordering

The user stories are independently testable, and P1 splits cleanly:

1. **Foundational** — the candidate rule and `EcosystemCandidate`, with the
   collision check (V5) and the member/merged guards (V3, V4). Testable with
   no network at all.
2. **US2 before US1.** US2 is "never invent a coordinate" — it is satisfied by
   the *absence* of a change, so implementing it first means the feature starts
   in its safe state and US1 opens it up under confirmation. The reverse order
   has a window in which every candidate is rewritten unconditionally.
3. **US1** — confirmation and the identifier rewrite, with the wiremock
   control that gives it teeth.
4. **US3** — C207 and C208, the catalog rows and the six parity extractors.
5. **Measurement and goldens** — SC-001's recorded run; the single moat golden
   refresh through `public-corpus.yml`.

## Risks

| Risk | Mitigation |
| --- | --- |
| The pass is moved out of the R11 window and orphans every corrected component | A test that fails if the pass runs after `closure::graph::relationships`; plus the before/after orphan-count check in quickstart Control 3 |
| A green corpus lane is read as evidence US1 works | Written into quickstart as the first thing; SC-007 requires a mutation check |
| Two candidates normalise to one coordinate (`Foo_Bar` / `foo-bar`) | V5 declines **both**; whether any such pair exists on the pinned closure is measured as a task, not assumed |
| Confirmation slows a scan | ≤75 lookups, batched, behind a two-level cache; zero on `--no-deps-dev` and on a warm cache |
| Drift into applying the returned `VersionInfo` (licenses, links) | Named an explicit non-goal in research R10; would change goldens no requirement predicts |
| SC-001 quoted from a prediction, or from the wrong system's closure | SC-001 forbids prediction; quickstart Control 2 pins the command, and both the spec and quickstart state the aarch64-darwin vs x86_64-linux hazard |

## Complexity Tracking

*No Constitution Check violations. Section intentionally empty.*

---

**Phase 0 complete** — `research.md`, 11 findings, all `NEEDS CLARIFICATION` resolved.
**Phase 1 complete** — `data-model.md`, `contracts/annotations.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit-tasks`.
