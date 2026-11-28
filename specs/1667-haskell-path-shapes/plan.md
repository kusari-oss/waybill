# Implementation Plan: Haskell path values have the right shape

**Branch**: `1667-haskell-path-shapes` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)
**Input**: specs/1667-haskell-path-shapes/spec.md (#1463, #1691)

## Summary

- **C200.** Every path in C200 is relative to the canonical scan root. That includes the include entries (R1), and paths above the root, which get `..` segments (R3). `manifests-dropped-by-scope` compares relative paths with relative paths (R4).
- **`source_dir`.** One function decides a subproject's directory: the parent when the path is a listed manifest name or a file on disk. Both `--split` and `--project-discovery` call it (R5).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde_json` and `std::path`. **Zero new Cargo dependencies.**
**Storage**: N/A. Everything is in process for one scan.
**Testing**: cargo-nextest; `waybill-fixture-*` three-format integration tests; recorded mutations (`measurements/mutate.py`); corpus goldens from the `public-corpus.yml` dispatch.
**Target Platform**: all three CI lanes. Path separators: the relative form uses `/` on every host, as the other per-file annotations do. To be confirmed against the Windows lane's existing tests.
**Project Type**: CLI, one crate (`waybill-cli`).
**Performance Goals**: no measurable change.
**Constraints**: C200's keys and order are unchanged (FR-003). The symlink test passes unchanged (FR-004).
**Scale/Scope**: 4 code sites:
- `haskell/hpack.rs`, `to_document_value(root)`;
- `haskell.rs`, `finalize`'s call and `amend_scope_dropped`'s unit test;
- `cli/scan_cmd.rs`, where the candidates join goes;
- `generate/split.rs` and `generate/project_discovery/mod.rs`, `subproject_dir`.

Plus one doc row: C200 in `docs/reference/sbom-format-mapping.md`.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | Pass: no dependencies. |
| IV. Type-driven correctness | Pass: the root is passed explicitly into the one function that builds C200. One function answers "subproject directory". |
| V. Specification compliance / standards-native first | Pass: no new row. C200 keeps its shape; only its path strings change. |
| VII. Test isolation | Pass: the fixtures are built in tempdirs with `waybill-fixture-*` names. |
| X. Transparency | Pass: the manifests read are still all named, now without host details. |
| Pre-PR gate | `./scripts/gate.sh … prepr -- ./scripts/pre-pr.sh`, default features. |

No violations, so Complexity Tracking is empty.

## Project Structure

```text
specs/1667-haskell-path-shapes/
├── spec.md  plan.md  research.md  data-model.md  quickstart.md
├── contracts/annotations.md
└── measurements/  (measure.sh, main/, branch/, mutate.py, mutations.md)

waybill-cli/src/scan_fs/package_db/haskell/hpack.rs      # R1–R3
waybill-cli/src/scan_fs/package_db/haskell.rs            # finalize, R4 test
waybill-cli/src/cli/scan_cmd.rs                          # R4
waybill-cli/src/generate/split.rs                        # R5
waybill-cli/src/generate/project_discovery/mod.rs        # R5
waybill-cli/tests/haskell_path_shapes.rs                 # FR-007 control
docs/reference/sbom-format-mapping.md                    # C200 text
```

**Structure decision**: there's no new module. `subproject_dir` lives in `split.rs` beside `source_dir_for`, which produces `source_dir`.

## Phase 0 / 1 outputs

- `research.md`, R1–R6.
- `data-model.md`: the two value forms.
- `contracts/annotations.md`: C200 and `split-manifest.json`'s `source_dir`.
- `quickstart.md`.
- Agent context: I'll add the line to `CLAUDE.md`'s Recent Changes by hand at implement time. Running `update-agent-context.sh` would delete earlier lines.

## Post-design Constitution re-check

The table above is unchanged.

## Complexity Tracking

None.
