# Implementation Plan: `fragment` requires evidence of a live-bootstrap function

**Branch**: `1503-fragment-positive-evidence` (base `main` at ac2bd28) | **Date**: 2026-11-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1503-fragment-positive-evidence/spec.md`

## Summary

The C206 classifier's last rung, "else `fragment`", becomes four rungs in this order:
1. **`fragment`**, when the derivation's `builder` is live-bootstrap's `kaem` (FR-001, research R1).
2. **`package`**, when the member is a `makeSetupHook` output, i.e. its `buildCommand` installs `nix-support/setup-hook` (FR-004, R2/R7, user decision).
3. **`scaffolding`**, when the member's name ends in `.sh`, `.bash` or `.pl` (FR-004, R2/R7).
4. **`package`** otherwise, the conservative default (FR-004, R3).

`package` is an edge endpoint. The projection therefore stops at `stdenv-linux`, the stdenv stages and the build utilities, instead of walking through them to the whole toolchain. The projection code does not change (FR-005).

The edge change is measured with the #1367 accounting on all five closures, which are captured in CI (FR-006, FR-007). It is then reviewed against the regenerated goldens (FR-010).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only (`serde`/`serde_json`). **Zero new Cargo dependencies.**
**Storage**: N/A, in-process per scan.
**Testing**: `cargo test`.
- Unit tests in `nix/closure/member.rs` use constructed closures.
- The env-gated `s1367` accounting runs on each captured closure.
- `debug closure-dump` provides the per-member listing.
- The public-corpus goldens are regenerated in CI.

**Target Platform**: as today. Measurement is x86_64-linux only.
**Project Type**: CLI (single binary).
**Performance Goals**: no measurable change. Two string tests per member that reaches the last rung.
**Constraints**: the five frozen C206 values. No new annotation. No identity change (R4). No closure evaluated locally (FR-006).
**Scale/Scope**: on moat, 25 of 1,749 derivations change role (10 → `scaffolding`, 15 → `package`). On openai and slack-web, all 67 and 66 fragments leave `fragment`. R7 fixes the counts for the other three closures.

## Constitution Check

| Principle | Assessment |
|---|---|
| III. Fail Closed | Pass. An unevidenced member is no longer labelled `fragment` by default. It falls to `package`, where the graph stops, so absence of evidence cannot fan a consumer out. |
| IV. Type-Driven Correctness | Pass. The role stays a closed enum over five values, and `MemberRole::ALL` is unchanged. The rung is an enum returned beside the role, so the role and the debug explanation cannot disagree (R6). |
| V. Specification Compliance | Pass. No new `waybill:` row; existing values change. |
| VII. Test Isolation | Pass. Unit tests construct closures. The real-closure measurement is env-gated, reads captures, and evaluates nothing. |
| VIII. Completeness | Pass. Members that become `package` gain the edges nix recorded to them, instead of being walked through. |
| IX. Accuracy | Pass, and the point of the feature: `fragment` stops being said of setup hooks and of `stdenv-linux`. |
| X. Transparency | Pass. The rung that decided each role is visible in `debug closure-dump`, and the measurement is committed. |

No violations, so Complexity Tracking is empty. Re-checked after Phase 1 design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1503-fragment-positive-evidence/
├── spec.md
├── plan.md
├── research.md            # R7 filled from the CI captures before /speckit-tasks
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/
    ├── probe/attrs.py     # joins closure-dump rows to builder/stdenv attributes
    └── <target>/          # created by the tasks: before/after dump + accounting, default-members.tsv
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── derivation.rs   # RawDerivation.builder (R5)
└── member.rs       # classify(): rungs 5–8; classify_with_rung() + MemberRung (R6); MemberRole docs
waybill-cli/src/cli/debug_cmd.rs        # closure-dump: member_role_rung
docs/reference/sbom-format-mapping.md   # C206 row
docs/reference/nix-evaluation.md        # role table, evidence order, the fragment sentence
CHANGELOG.md
waybill-cli/tests/fixtures/public_corpus/nix-closure-*/   # regenerated in CI
```

**Structure Decision**: Single project. Two closure-tier files and one debug command change, plus docs and goldens. No new module.

The projection (`graph.rs`) and the emitter (`emit.rs`) are not edited. Both reach the role through `member::is_edge_endpoint` and `member::classify`, and the new evidence lives on the derivation (R3).

## Approach

1. **Parse.** Add `builder: Option<String>` to `RawDerivation` with `#[serde(default)]`.
2. **Classify.**
   - Introduce a `MemberRung` enum with the eight rungs and their kebab-case `wire()` strings.
   - Introduce `classify_with_rung(drv, patches) -> (MemberRole, MemberRung)` holding the evidence order. `classify` becomes `classify_with_rung(..).0`.
   - The last rung tests, in this order:
     - `builder`'s final path component `== "kaem"` → `Fragment`;
     - the name ends with `.sh` or `.bash` → `Scaffolding`;
     - otherwise → `Package`.
   - Update the `MemberRole` variant docs and the evidence-order doc comment.
3. **Debug.** Add `member_role_rung` to `closure-dump` rows.
4. **Tests.**
   - **Audit every existing test** that asserts `MemberRole::Fragment` on a constructed derivation with no `builder`, for example `{"name":"strncpy","env":{}}`. Each one now gets `package`. Each is rewritten either to carry a `kaem` builder (where it tests something else) or to assert the new default (where it tests the last rung).
   - **The spec-1380 equivalence tests** (`classify_before_1380`) compare against an older classifier. Re-establish which population they cover, and give the old classifier the same last rung where the test means "the version rung is unchanged".
   - **Add `s1503` tests:** the four new rungs, the `stdenv-linux` endpoint case, and the earlier rungs outranking the new ones (FR-002).
   - **Control (SC-005):** a mutation that drops the builder test turns the bash-built fixture member back into `fragment`. Record it in `measurements/mutations.md`. Revert with `cp`.
5. **Measure (FR-006, FR-007).** For each of the five captures:
   - run `closure-dump` and the `s1367` accounting on ac2bd28 and on the change;
   - list the members that change role, and the `default` members;
   - commit them under `measurements/<target>/` with a delta write-up.

   The captures come from public-corpus run 43629708718, on the branch-only `1503-closure-capture`. That branch is never merged and is deleted after the artifacts are saved.
6. **Goldens.**
   - Dispatch the corpus with `regen_goldens=true` on this branch.
   - Run a normalized, sorted diff per target, reviewed member by member and edge by edge against step 5.
   - Commit the goldens with `measurements/golden-review.md`.
7. **Docs.** The C206 row, `nix-evaluation.md` and the CHANGELOG (not adjacent to another unreleased entry).
8. **Gate.** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.

## Resolved during implementation

- **R7.** It was settled by tasks T005 and T006 from the CI captures. R1 held. R2 gained the setup-hook rung and `.pl`, by the user's decisions recorded in the spec's Clarifications.

## Complexity Tracking

None.
