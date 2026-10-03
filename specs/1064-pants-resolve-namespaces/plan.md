# Implementation Plan: Pants resolves owned and named across both language namespaces

**Branch**: `1064-pants-resolve-namespaces` | **Date**: 2026-10-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1064-pants-resolve-namespaces/spec.md`

## Summary

Extend the Pants resolve-ownership model (m868 / #911 / m922) from Python-only to
both language namespaces:
- name unconfigured default resolves as Pants does (`jvm-default`,
  `python-default`);
- qualify owning-component identity with `?pants-namespace=`;
- give declared JVM resolves owning components built from the coursier lockfile's
  `generated_with_requirements`;
- make `waybill:resolve-ownership` (C161) one repository-wide statement with
  namespace-qualified names.

All four are consumer-visible and intended (FR-015). The approach mirrors the
Python reader's existing anchor and summary code. Research found one adaptation:
JVM tools declare lockfiles through `[<scope>].lockfile` (a path) rather than
`install_from_resolve` (a name).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only: `toml` (pants.toml), `serde`/`serde_json` (annotation values), `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. All state is in-process per scan.
**Testing**: `cargo +stable test --workspace` via `./scripts/pre-pr.sh`. New crate fixtures under `waybill-cli/tests/fixtures/pants_*`. Public-corpus goldens are regenerated through the CI lane.
**Target Platform**: Linux, macOS, Windows (no platform-specific code).
**Project Type**: CLI (`waybill-cli` crate).
**Performance Goals**: No measurable change. The added work is one extra pants.toml read per JVM reader invocation plus one owning component per declared JVM resolve.
**Constraints**: Byte-identical output for repositories with no Pants lockfiles (FR-014). Deterministic ordering (FR-011). Format parity on C161 (FR-012).
**Scale/Scope**: 2 readers (`pants/`, `pants_jvm/`), 1 merge site (`package_db/mod.rs`), 1 split lookup (`generate/split.rs:340`), 4 corpus targets, about 4 crate test files.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | ✅ No new crates, no C. |
| III. Fail closed | ✅ Unchanged posture. pants.toml / lockfile parse failures keep the readers' existing fail-open FR-004 behaviour, which is scoped to *discovery*. No new path silently invents a declaration: a missing or unreadable pants.toml means no built-in default (R2). |
| IV. Type-driven correctness | ✅ Namespace stays the existing `LanguageNamespace` enum. The new `Declaration` variants are an enum, not strings. |
| V. Specification compliance | ✅ `pants-namespace` is a valid PURL qualifier key (lowercase ASCII letters and `-`). Its non-registration is documented (#1106), not hidden. |
| VII. Test isolation | ✅ New fixtures use synthetic `waybill-fixture-*` / `dev.waybill.fixture` coordinates. Corpus targets are pinned public SHAs. |
| VIII. Completeness | ✅ The point of the feature: JVM resolves gain an owner and a statement. |
| IX. Accuracy | ✅ Anchors only for *declared* resolves. Discovered-by-convention lockfiles stay unanchored (m868 FR-003 kept). Edges come from the lockfile's own declaration, not inferred from graph shape. |
| X. Transparency | ✅ C161 now covers every namespace, and the heuristic-classification count includes JVM resolves. |
| XI / XII. Enrichment | ➖ Not touched. |

No violations, so Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/1064-pants-resolve-namespaces/
├── plan.md              # This file
├── research.md          # Phase 0: R1–R7
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   ├── resolve-ownership.md   # C161 v2
│   └── anchor-identity.md     # owning-component PURL
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit.tasks)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── pants/
│   ├── mod.rs              # built-in default naming (R2); summary keeps bare names, merge qualifies
│   ├── config.rs           # "resolves table absent" signal for R2
│   └── lockfile.rs         # anchor PURL gains ?pants-namespace=python (R1)
├── pants_jvm/
│   ├── mod.rs              # declarations (R2, R5), anchors (R3), summary (R4)
│   ├── config.rs           # parse other tables' `lockfile` keys (R5); "resolves absent" signal (R2)
│   ├── lockfile.rs         # use generated_with_requirements; jvm anchor builder (R3)
│   └── resolve_classifier.rs  # declared-by-tool → Development/Declared (R5)
├── pants_resolve.rs        # qualified-name helper shared by the merge
└── mod.rs                  # merge Python + JVM summaries into one C161 value (R4)
waybill-cli/src/generate/split.rs   # anchor lookup matches namespace (R1)

waybill-cli/tests/
├── fixtures/pants_coursier_jvm/   # new: implicit-default, tool-lockfile, top-level-requirements fixtures
├── pants_coursier_jvm_reader.rs   # JVM anchors, naming, tool classification, C161
├── pants_namespace_split.rs       # collision: two anchors, split still correct
├── pants_resolve_membership.rs    # C161 v2 values
├── corpus_harness_195/layer1_assertions.rs  # qualified anchor match; I3 list update (SC-002)
└── fixtures/public_corpus/{pants-example-python,pants-example-django,pants-example-jvm,pants-clojure-polyglot}/  # regenerated via CI
```

**Structure Decision**: Single crate (`waybill-cli`), changes confined to the two
Pants readers, their shared `pants_resolve` helper, the diagnostics merge site,
and one split lookup. Emitters are untouched (R6).

## Delivery order

1. **US2 identity first** (R1 + split lookup). No new anchors yet; Python anchors
   gain the qualifier. Corpus: Python anchor identifiers change.
2. **US3 naming** (R2) for both readers.
3. **US1 JVM anchors + statement** (R3, R4). Verify R6 (the clojure target at
   4 / 4 / 4) before closing.
4. **US4 tool lockfiles** (R5).
5. Corpus regeneration (twice, compared), CHANGELOG entry (FR-015), and the
   catalogue note superseding m912 SC-006.

Steps 1–2 must land before 3. Without qualified identity, JVM anchors would
merge in collision repositories (spec US2 rationale). Without Pants-default
naming, `pants-example-jvm` (no `[jvm.resolves]`) would stay *discovered* and
get no anchor, so SC-001 needs step 2. tasks.md follows this order, and ships
steps 2 and 3 in one PR so that `pants-example-jvm`'s goldens change once.

## Complexity Tracking

None.
