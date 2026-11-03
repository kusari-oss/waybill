# Implementation Plan: the nix-closure tier says why it degraded

**Branch**: `1274-nix-degradation-reason` | **Date**: 2026-11-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/1274-nix-degradation-reason/spec.md`

## Summary

`--nix-closure` refuses import-from-derivation on purpose (#1034, #1114), and
`callCabal2nix` — the common way to build a Haskell project with Nix —
requires it. So the tier degrades on those projects by design and the document
does not say so: it records `evaluation-failed`, the catch-all that also
covers a syntax error and a nixpkgs type error. An operator concludes waybill
is broken on their repository.

**Technical approach.** Three changes, each the smallest thing that satisfies
its requirement:

1. **One word in the salient-line rule.** `position` → `rposition`. Phase 0
   measured that Nix prints the stack trace first and the cause last, so the
   existing rule fills its 300-character window with trace and never reaches
   the explanation.
2. **One new `DegradationReason` variant**, classified by Nix's consequence
   clause on an error line — both conditions, because a flake that merely
   *requests* the setting puts its name in stderr.
3. **C190's value becomes `{reason, detail}`**, so the artifact can answer
   "why" without the log. An existing row with three extractors, so no
   catalog row is added.

The rule moves rather than being copied, because a second copy is the defect
this milestone fixes.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.

**Primary Dependencies**: existing only — `serde_json` (the object value),
`tracing` (the log half of FR-003). **Zero new Cargo dependencies.** External
runtime dependency on `nix` is unchanged: the tier already requires it and
this feature adds no invocation.

**Storage**: N/A. In-process per scan; the only durable output is C190's value
in the emitted document.

**Testing**: `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace --no-fail-fast`, via
`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
No `+toolchain` (#1189). Mutation-tested per FR-015.

**Target Platform**: Linux, macOS, Windows for the build; the tier itself
needs `nix`, which it already declares.

**Performance Goals**: none. The change is a string selection and a predicate.

**Constraints**: no CLI flag and the refusal stays non-configurable (FR-011);
no behaviour varies with host `PATH` beyond the existing `nix` dependency
(FR-012); a flake that evaluates today produces byte-identical output
(FR-010, SC-003).

**Scale/Scope**: 21 functional requirements, 8 success criteria, 13
document/CLI contracts. Measured blast radius: **0** corpus goldens, **6**
assertions in one integration suite.

**NEEDS CLARIFICATION**: none. The spec entered Phase 0 with zero markers
after three clarifications, and Phase 0's own obligation — pinning Nix's
wording — is discharged in R1/R2/R5.

## Constitution Check

*Constitution v4.0.1. Evaluated before Phase 0 and re-evaluated after Phase 1.*

| principle | verdict | note |
|---|---|---|
| **Operating Modes** | PASS | Scan mode throughout. `waybill:generation-context` unchanged. |
| **I. Pure Rust, Statically Linked** | PASS | No new dependency of any kind. |
| **II. eBPF-Only Observation** | N/A | Trace-mode principle. |
| **III. Fail Closed** | PASS | Directly served. The scan-mode analogue is *report, never pad*, and the tier already degrades correctly — this makes the degradation legible. FR-010 forbids changing what degrades. |
| **IV. Type-Driven Correctness** | PASS | A new enum variant in a closed set, not a string flag. FR-013 keeps the set's existing members intact. |
| **V. Specification Compliance** | PASS — **audited** | See below. No new `waybill:*` key, no new catalog row. |
| **VI. Three-Crate Architecture** | PASS | `waybill-cli` only. |
| **VII. Test Isolation** | PASS | The probe builds its fixtures in `mktemp -d` and `git init`s them. It does need a real `nix` and the nixpkgs store — which is the tier's own dependency, and the reason the probe lives under `measurements/` rather than in the test suite. |
| **VIII. Completeness** | PASS | Unaffected: the component set does not change (FR-010). |
| **IX. Accuracy** | PASS — **and the sharp edge** | FR-005/FR-005a exist because the obvious classifier mis-reports a missing attribute as a policy refusal. Trading one uninformative answer for one misleading answer would breach this principle, which is why the false positive was measured before the anchor was chosen. |
| **X. Transparency** | PASS | This feature *is* Principle X: "when waybill cannot guarantee completeness, it MUST include structured metadata informing the consumer of the limitation." Today the metadata exists and does not inform. |
| **XI. Enrichment** | N/A | No enrichment. |
| **XII. External Data Source Enrichment** | PASS | Constraint 2 (provenance): the detail is Nix's own words, attributed to the tier that ran it. Constraint 3 (degrade gracefully with transparency annotations) is the whole feature. |
| **Strict Boundary 1** | N/A | Scan mode. |
| **Pre-PR Verification** | PASS | Both commands, no `+toolchain`, via `gate.sh --expect`. |

### Principle V — standards-native audit (cited, as the principle requires)

| signal | native construct? | carrier |
|---|---|---|
| why a requested tier declined | **No.** CycloneDX has no field for "an optional analysis tier declined and here is why"; `compositions[].aggregate` describes the composition's completeness, not a tool's decision about its own inputs. SPDX 2.3 and 3.x have no analogue | existing `waybill:nix-closure-degraded` (**C190**), value extended |
| the explanatory text | **No.** Same gap; no format carries a tool's diagnostic prose | the same annotation's `detail` field |

**Result: no new `waybill:*` property and no new catalog row.** C190 exists
with three extractors; FR-002b records that extending a row is not adding
one, so FR-014's extractor condition is not triggered. The row's documented
prose does change — it currently says "document-scope string" — which
`mapping_doc_bidirectional` enforces.

### Post-Phase-1 re-evaluation

Re-checked after `data-model.md` and `contracts/README.md`. **No new
violations.** One gate tightened rather than loosened: the Principle IX row
above was written *after* R2 measured the false positive, which turned FR-005
from a sensible caution into a specific, named case with a committed
reproduction. The gate found the shape of the risk, not merely its existence.

No deviations to record.

## Project Structure

```text
specs/1274-nix-degradation-reason/
├── plan.md                      # this file
├── spec.md                      # 21 FRs, 8 SCs, 0 markers, 4 clarifications
├── research.md                  # R1–R6
├── data-model.md
├── quickstart.md
├── contracts/README.md          # C1–C13
├── checklists/requirements.md   # 16/16
└── measurements/
    └── reproduce-1283.sh        # two flakes + a control
```

```text
waybill-cli/src/scan_fs/package_db/nix/
├── eval/
│   ├── invoke.rs     # salient_stderr: position -> rposition; the rule moves out
│   ├── reason.rs     # + ImportFromDerivationRequired
│   └── mod.rs        # the rule's new home, reachable from both paths
└── closure/mod.rs    # :310 stops keeping a blind 200-char prefix

waybill-cli/src/generate/
├── cyclonedx/metadata.rs     # C190 value: string -> object
└── spdx/{annotations.rs, v3_annotations.rs}

waybill-cli/tests/nix_config_closure.rs   # 6 assertions corrected
docs/reference/sbom-format-mapping.md     # C190's prose
```

**Structure decision**: the salient-line rule moves to a home both paths can
reach rather than being duplicated. `guard()`, two definitions below the
drift, already carries the comment *"two copies would drift, and the drift
would be exactly the thing the guard prevents"* — about the argv guard. The
error reporting drifted instead, and copying the fix would set up the next
one.

## Phase 0 — research (COMPLETE)

Output: [`research.md`](./research.md), `measurements/reproduce-1283.sh`.

| # | finding |
|---|---|
| **R1** | **The spec's central assumption was wrong.** It assumed `salient_stderr`'s rule was right and merely unapplied. Measured: Nix emits the trace first and the cause last, so the rule's forward window from the *first* error line misses the clause by 1,155 characters. `position` → `rposition` fixes both paths; verified on both probe cases |
| **R2** | The FR-005a false positive is real: a flake requesting the setting and failing on a missing attribute puts the setting's name in a *warning*. `slack-web` sets it |
| **R3** | C190 exists with three extractors, so no catalog row is added — FR-002b holds |
| **R4** | **Blast radius is not where the spec implied.** 0 corpus goldens; **6 assertions** in `nix_config_closure.rs` |
| **R5** | Nix wording pinned to Determinate Nix 3.20.0, the same installer family CI uses. Deliberately *not* verified across versions — one version is not a range, and FR-005b already requires degrading on a rewording |
| **R6** | No new surface: no dependency, flag, subprocess or network |

**R1 is the reason this plan differs from the spec it implements.** FR-007 and
FR-008 read as "apply the existing rule to the second path"; that would have
moved the defect. The spec's own Assumptions section predicted the shape of
this ("if the rule is wrong, it is wrong in both places") without expecting it
to be true.

## Phase 1 — design (COMPLETE)

Output: [`data-model.md`](./data-model.md),
[`contracts/README.md`](./contracts/README.md),
[`quickstart.md`](./quickstart.md).

| decision | measurement that forced it |
|---|---|
| `rposition`, not a wider window | R1 — the cause is at byte 1320 of 1382; widening would put a kilobyte of trace in an SBOM |
| match the clause, on an error line | R2 — the bare setting name appears in a warning on an unrelated failure |
| precedence is an explicit search, not a by-product of selection | FR-005c — with `rposition` the last error line may be the *other* error, so the refusal has to be looked for |
| extend C190, add no row | R3 — three extractors already exist |
| correct 6 assertions, churn 0 goldens | R4 |

### Risks carried deliberately

| risk | guard |
|---|---|
| **A text match on another tool's output.** Nix could reword the clause | FR-005b/FR-006 make a rewording degrade to the generic reason; the probe is committed so the match is re-verifiable. Accepted because the alternative — no classification — is the status quo |
| **The wire shape changes.** External bare-string consumers break | Stated in the spec rather than discovered (FR-002a). In-repo population measured: 6 assertions, 0 goldens |
| **A policy note could mask a flake bug** | FR-005c keeps both lines in the detail |
| **The new reason is confusable with `ifd-refusal-unverified`** | C10 asserts that reason keeps its meaning; they are near-opposites — "could not confirm the refusal is in effect" versus "the refusal took effect" |
| **One version of Nix measured** | Recorded as a limit in R5 rather than presented as a range |

## What `/speckit.tasks` must satisfy

1. **`rposition` lands before the classifier.** R1 shows the classifier
   cannot match the clause while the selection returns a bare `error:`
   opener, so a classifier task sequenced first would be untestable.
2. **The rule moves before the closure path uses it.** Copying it is the
   defect.
3. **A negative test for the classifier**, using R2's measured false
   positive. FR-015 names it specifically because an over-matching classifier
   is the likely way to get this wrong.
4. **Assertions on the clause, not on "not the warning".** C4's whole point:
   a weaker assertion passes on the stack trace.
5. **The 6 `nix_config_closure.rs` assertions corrected, not pinned**, and
   that suite is where FR-013 is proved.
6. **A baseline captured before the first change** (FR-016), and a check that
   0 corpus goldens change.
7. **Mutation steps per requirement**, with `measurements/mutate.py` from
   milestone 1273 available — it refuses to report a verdict for a mutation
   it could not apply.

## Stop

Planning complete. `/speckit.tasks` next.
