# Implementation Plan: A requirements file belongs to the project in its directory, not to a lockfile entry beside it (#1640)

**Branch**: `1642-requirements-owner-editable` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

## Summary

Two defects, both measured at `main` `79b3350b` (`measurements/probe.md`):

1. **The owner of a directory follows package-name order.** When the pip main-module pass merges a `pyproject.toml` main module into a lockfile entry of the same PURL, it keeps the lockfile's source path. Two editable packages from one `uv.lock` then both claim the lock's directory, and the lower PURL wins (research R1).
2. **A root-declared package can be reported as not shipping.** A scan-root requirements file's packages reach the root through reader root anchors, and spec 1269's immunity search does not start from anchors. A non-runtime path into such a package then gives it a transitive non-runtime scope (research R4).

**The approach:**
- **R2:** the merge keeps the manifest's directory first, with the lockfile path after it. `OwnerIndex` and `waybill:workspace-member` follow. Installed entries are exempt, to keep `waybill:layer-digest` (planning amendment).
- **R3:** `OwnerIndex`'s key becomes `(rank, non-runtime, PURL)`, and a tie the PURL decides is logged. FR-003's step 1 holds by construction after R2 (planning amendment).
- **R5:** the reader root anchors that are runtime join spec 1616's added seeds, in every ecosystem. Only the immunity search widens.
- **R6, a census first:** a log-only commit and a CI workflow count owner ties, merges, dev-only main modules and the components FR-005 would protect, on TheRock and the 18 quality-corpus targets, before the rule is wired. The public corpus is measured by its regenerated diff (planning amendment).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- Code:
  - `scan_fs/package_db/pip/mod.rs` (the augment-existing branch);
  - `scan_fs/package_db/pip/uv_lock.rs` (carry the editable path for R2's cross-check);
  - `scan_fs/declared_attach.rs` (`OwnerIndex`);
  - `resolve/scope_propagation.rs`;
  - `cli/scan_cmd.rs` (the anchor list, the seeds).
- Crates: `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-009).

**Storage**: N/A.

**Testing**:
- **Unit:** the R2 merge (lockfile and installed), the R3 key order, and the R5 anchor seeds (runtime and non-runtime anchors).
- **Integration:** `waybill-cli/tests/requirements_owner.rs`, with the cases in `contracts/wire.md`. Each case checks edges, scope, C18, C120 and C62 across the three formats, parity-check, CycloneDX validity, unique `bom-ref`s, and 10 byte-identical scans.
- **Control:** the cases land at the log-only commit. Every case that this spec changes must fail there.
- **Mutations** (research R7): restore with `cp`, rebuild after the last one, and use an `^error\[E` guard.
- **CI:**
  - the census (`census-owners.yml`, fixture repository) at the log-only commit;
  - the public corpus, `regen_goldens`, at the log-only commit (0 differing expected) and at the final commit;
  - the quality corpus against `main`;
  - TheRock `measure-source` against `main`.

**Target Platform**: the waybill CLI on Linux, macOS and Windows.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-005, at most 1.05× `main` on TheRock, median of at least 3. The changes are a field swap per merge, one more key field per candidate, and a few more BFS seeds.

**Constraints**:
- FR-007: byte-identical output when every directory has one owner candidate and FR-005 removes no scope;
- installed entries keep their first source path;
- graph completeness and the FR-012 gate keep spec 1616's `seeds`.

**Scale/Scope**: about 150 lines of production code across five files, plus a test file and the census.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| IV. Type-driven correctness | Pass. The key is a typed tuple. No `.unwrap()` in production. |
| V. Specification compliance | Pass. No new row. Scope stays native (CycloneDX `scope`, SPDX relationship types), and C18/C120/C62 keep their meaning. |
| VIII. Completeness / IX. Accuracy | This is the point of the change. A false edge from a dev tool is removed, and root-declared runtime requirements stop being reported as not shipping (spec 1269's dangerous direction). |
| X. Transparency | Pass. C18 keeps the lockfile path after the manifest directory. The propagation summary counts what anchors protected. A PURL-decided tie is logged. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1642-requirements-owner-editable/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
├── measurements/          (probe.sh, probe.md; later census, control, mutations, gate, corpus, therock)
└── tasks.md               (/speckit-tasks)
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/pip/mod.rs        # R2: augment-existing keeps the manifest dir first (non-installed)
waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs    # R2: editable path carried for the cross-check
waybill-cli/src/scan_fs/declared_attach.rs           # R3: (rank, non_runtime, purl); PURL-tie log
waybill-cli/src/resolve/scope_propagation.rs         # R5: anchor seeds; immune_by_root_anchors
waybill-cli/src/cli/scan_cmd.rs                      # R5: keep the anchor list; pass it to the seeds
waybill-cli/tests/requirements_owner.rs              # integration cases
docs/reference/sbom-format-mapping.md (C18, C62, C120), docs/reference/reading-a-mikebom-sbom.md, CHANGELOG.md
```

Fixture repository (`kusari-sandbox/waybill-fixture-therock-sdk`): `census/owners.py`, `.github/workflows/census-owners.yml`.

## Complexity Tracking

None.
