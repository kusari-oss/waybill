# Implementation Plan: graph completeness counts a partial closure read

**Branch**: `1656-closure-read-completeness` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)
**Input**: `specs/1656-closure-read-completeness/spec.md`. Refs #1720, #1348, #1696.

## Summary

**One predicate, typed (R1).** The emitters learn about a partial closure read from one function, `closure_read_gaps(read_outcome, contents)`. Its `any()` replaces both copies of the "closure read was partial" rule:
- `ScanArtifacts::closure_read_partial()`;
- the CycloneDX builder's inline expression.

Its counts feed two new C105 codes, `closure-declarations-unconsulted` and `closure-contents-unread`. These are appended by a post-pass that mirrors spec 1652's `apply_scope_drop` (R2), and they degrade no ecosystem (R3). The `nix_closure_contents_unread: bool` becomes `Option<ContentsUnread>`, so the counts reach the emitters without the C213 string being parsed back.

**Expected effect (R5):**
- shellcheck's C104 goes from `complete` to `partial`;
- each closure golden's C105 gains the two clauses with its measured counts;
- nothing else changes, in any golden.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. **Zero new Cargo dependencies.**
**Storage**: N/A. The inputs are already computed in process.
**Testing**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
- Unit tests in `graph_completeness` cover the codes, their formatting, the value rule, the shared predicate, and no ecosystem being degraded.
- Integration tests on the closure fixtures cover: a declaration gap (`closure_structured_attrs`), a contents gap only (the stub), neither (`closure_binary_contents`, R4), and three-format parity through `assert_parity`.
- One mutation per new behaviour.
- One corpus dispatch, with research R5's expectations scored against the diff.

**Target Platform / Project Type**: CLI, single Rust workspace; the closure fixtures are Unix-only, as today.
**Constraints**: non-closure documents are byte-identical, and `compositions[]` and SPDX `completeness` are byte-identical everywhere.
**Scale/Scope**: 2 reason variants, 1 post-pass, 1 shared function, about 14 call sites changed from a bool to an `Option`.

## Constitution Check

| Principle | Check | Status |
|---|---|---|
| I. Pure Rust | No new dependency. | Pass |
| III. Fail closed | A gap the reader recorded now makes C104 `partial` instead of being silently ignored. | Pass |
| IV. Type-driven | The bool becomes a typed `ContentsUnread`; the gaps are a struct, not a string re-parsed. | Pass |
| V. Specification compliance | C105 is an existing row; its vocabulary grows, and the row and the vocabulary contract are updated (FR-006). The Principle V native-construct audit was settled by milestone 1278: native `compositions` and SPDX `completeness` already carry the fact; C104/C105 are waybill's own verdict. | Pass |
| IX. Accuracy | This is the point: removes a `complete` that the document's own disclosures contradict. | Pass |
| Measure first | Every expected value in R5 is from `measurements/read-gaps-before.txt`. | Pass |

No violations. The post-design re-check is unchanged.

## Project Structure

```text
specs/1656-closure-read-completeness/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md, contracts/wire.md
├── checklists/requirements.md
└── measurements/  read_gaps.py, read-gaps-before.txt, after.md (implement time)

waybill-cli/src/generate/graph_completeness/reason_codes.rs   # two variants + wire text
waybill-cli/src/generate/graph_completeness/mod.rs            # ClosureReadGaps, closure_read_gaps, apply_closure_read
waybill-cli/src/generate/graph_completeness/document.rs       # for_document gains closure_read
waybill-cli/src/generate/mod.rs                               # ScanArtifacts field type; closure_read_partial() via the shared fn
waybill-cli/src/generate/cyclonedx/{builder.rs,mod.rs,compositions.rs}  # field type, shared predicate, degraded_ecosystems arms
waybill-cli/src/generate/spdx/{document.rs,v3_document.rs}    # pass closure_read
waybill-cli/src/generate/{openvex/mod.rs,spdx/packages.rs,spdx/mod.rs,spdx/relationships.rs,spdx/document.rs}  # literal false -> None
waybill-cli/src/generate/orphan_reason.rs                     # passes None (unchanged behaviour)
waybill-cli/src/cli/scan_cmd.rs                               # ClosureContentsSummary carries counts
waybill-cli/tests/{closure_structured_attrs,closure_binary_contents,boot_library_edges}.rs
docs/reference/sbom-format-mapping.md (C105), specs/158-graph-completeness/contracts/graph-completeness-vocabulary.md, CHANGELOG.md
```

## Phases

- **Phase 0:** research R1 to R5, complete.
- **Phase 1:** design artifacts, complete.
- **Phase 2:** `/speckit-tasks`.

## Complexity Tracking

None.
