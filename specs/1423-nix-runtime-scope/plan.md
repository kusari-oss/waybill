# Implementation Plan: Nix closure scope from the runtime closure

**Branch**: `1423-nix-runtime-scope` | **Date**: 2026-11-10 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1423-nix-runtime-scope/spec.md`

## Summary

Today a `--nix-closure` document's not-in-artifact marks come from a build-graph role. Scored against nix's runtime closure, they are 31% precise as a findings filter. This feature replaces them with:

- **Runtime truth:** `nix path-info -r` of the outputs the evaluated attribute installs, falling back to its default output path (research R2, revised in US1 and again for #1458). It reads the local store first, then the substituters the user already trusts. Remote answers are signature-verified, and every query has a connect timeout and a single download attempt.
- **Static linking:** components linked into a runtime member also count as runtime. Go and Rust modules come from the milestone-1378 embedded readers. Haskell libraries come from the milestone-1275 declared edges, or, when those are unavailable, from derivation fields.
- **Fallback:** when no store answers, a reachability walk from the evaluated package.

The method is recorded per component in C203 (three new values) and per document in a new row, C216 `waybill:closure-scope-method`. `sbom edit` gains `scope=excluded`, which selects the same set in all three formats, and the runtime-only recipe is `sbom edit --drop 'scope=excluded'`.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only.
- `serde`/`serde_json` for C216 and C203.
- `tracing`, `anyhow`.
- The `eval::invoke` helpers: `run_bounded`, `argv_is_safe`/`guard`.
- `RawClosure::output_index`, `contents::MemberKey`, and the m1275 `nix-closure-declared` relationships.
- External: the host's `nix`, already required by `--nix-closure`.

**Zero new Cargo dependencies** (FR-013).
**Storage**: N/A. The runtime closure is in process for one scan; nothing persists beyond the emitted document.
**Testing**: `cargo test`.
- Hermetic stub-`nix` tests (`tests/common/nix_stub.rs` gains `path-info` and `store verify` branches).
- Unit tests in `closure_role_scope.rs` and `edit/`.
- A corpus target `nix-closure-shellcheck` for the live path (FR-016). The corpus is `#[ignore]` locally.
**Target Platform**: Every host `--nix-closure` supports (Linux, macOS). Behaviour is the same on both; closures differ by platform, as measured (hlint: darwin 322 vs linux 220 paths).
**Project Type**: CLI (`waybill sbom scan`, `waybill sbom edit`).
**Performance Goals** (SC-005):
- ≤ 0.1 s per runtime path on a cold remote query (measured: 34 ms per path);
- ≤ 5 s on an unreachable substituter (measured: 2.1 s with the bounds);
- signature verification still to be measured (research R11).

**Constraints**: Network goes only to substituters the user's nix already trusts (FR-004, FR-014). The closure tier does not run under `--offline` (R7). The closure tier's 300 s per-call budget applies.
**Scale/Scope**: Closures of 700–2,500 derivations; runtime closures of 3–322 paths.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, statically linked | Pass | No new dependencies; shells out to the host's `nix`, as the tier already does. |
| II / III (trace mode only) | N/A | Scan mode. |
| IV Type-driven correctness | Pass | `ScopeEvidence`, `RuntimeClosure` and `ScopeMethod` are enums or newtypes, not strings. No `.unwrap()` in production. |
| V Specification compliance, native fields first | Pass | The not-in-artifact mark stays native in all three formats: CDX `scope: excluded`, SPDX 2.3 `BUILD_TOOL_OF`, SPDX 3 `usesTool` with scope `build`. The audit in research R5 found no native document-scope field for "how scope was decided", so C216 is a `waybill:*` row with that justification. C203 is an existing parity-bridge row. |
| VII Test isolation | Pass | Stub `nix`; no network in the normal lanes (#1424). Live behaviour is checked only in the nightly corpus job. |
| IX Accuracy | Pass | Exact where nix proves it. The fallback errs toward runtime, never toward dropping linked code. |
| X Transparency | Pass | The method, the store that answered (or why none did), the static-link source and the unmatched-path count are all recorded (C216), plus a per-component C203. |
| XII External sources | Pass | Substituters are the user's own trusted caches, and answers are signature-verified. Provenance is recorded in C216. No new components are added; this only classifies existing ones. |

No violations, so the Complexity Tracking section is omitted.

**Re-check after Phase 1:** still passes. C216 has a catalog row and extractors in all three formats (data-model.md), and I10 is extended rather than weakened.

## Project Structure

### Documentation (this feature)

```text
specs/1423-nix-runtime-scope/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── c216-closure-scope-method.md
│   ├── c203-values.md
│   └── sbom-edit-scope-excluded.md
├── measurements/          # research probes (README.md in each)
└── tasks.md               # /speckit.tasks
```

### Source Code (repository root)

```text
waybill-common/src/resolution.rs                       # ScopeEvidence enum beside ArtifactPresence
waybill-cli/src/scan_fs/package_db/nix/closure/
├── runtime.rs        # NEW: query local store + substituters, verify, parse, join to members
├── static_link.rs    # NEW: Haskell static-link sets (declared edges, else field rule)
├── mod.rs            # wire runtime evidence into ClassifiedClosure; argv builders
└── summary.rs        # C216 wire value in NixClosureSummary
waybill-cli/src/resolve/closure_role_scope.rs          # decide presence from RuntimeEvidence; C203 values
waybill-cli/src/scan_fs/package_db/nix/closure/contents.rs   # stop deciding module presence; leave it to the pass
waybill-cli/src/cli/scan_cmd.rs                        # pass closure + relationships to the pass
waybill-cli/src/generate/cyclonedx/metadata.rs, spdx/annotations.rs, spdx/v3_annotations.rs  # C216
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs  # C216 extractors
waybill-cli/src/edit/{select,cdx,spdx23,spdx3}.rs      # Scope::NotInArtifact, scope=excluded
docs/reference/sbom-format-mapping.md                  # C216 row; C203 enum values
docs/user-guide/sbom-edit.md                           # runtime-only recipe
waybill-cli/tests/common/nix_stub.rs + 13 private stubs  # path-info / store verify branches
waybill-cli/tests/closure_runtime_scope.rs             # NEW integration tests
waybill-cli/tests/corpus_harness_195/{manifest,expectations}.rs, invariants/checks/closure_role_scope.rs
```

**Structure Decision:** The runtime query and the static-link computation are new modules in the closure tier, beside `contents.rs`, because that is where the store-path joins live. The decision stays in the one existing pass (research R1). `contents.rs` stops assigning presence, so exactly one place decides it.

## Phasing

1. **Foundation:** the `ScopeEvidence` type; stub-`nix` branches in every test stub (R9); the C216 catalog row and extractors.
2. **US1 (P1, MVP):** runtime query, signature verification, join, static linking, the decision in the pass, C203 values, I10, and the R11 measurement.
3. **US2 (P2):** reachability walk fallback; C216 records the reason.
4. **US3 (P2):** `scope=excluded` in `sbom edit` across the three formats.
5. **US4 (P3):** recipe documentation; scanner check on the five targets (SC-001, SC-002).
6. **Polish:** corpus target (FR-016; needs the maintainer's go-ahead for the wrapper-flake repo), CHANGELOG, golden regeneration.
