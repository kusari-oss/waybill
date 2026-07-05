# Implementation Plan: pnpm v9 multi-version edge disambiguation

**Branch**: `164-pnpm-multi-version-edges` | **Date**: 2026-07-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/164-pnpm-multi-version-edges/spec.md`

## Summary

Fix pnpm-lock v9's `collect_pnpm_dep_names` helper at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:80` where `parse_pnpm_key` returns `(canon_name, canon_ver)` but the version is immediately discarded with `_canon_ver`. The consequence: `snapshots_lookup` values are `Vec<String>` of bare names, which flow into each `PackageDbEntry.depends`, which the downstream edge resolver at `scan_fs/mod.rs:729-731` looks up name-only in `name_to_purl` — last-write-wins between multiple emitted versions of the same package. Result: 435 of 568 orphan components on live podman-desktop (2026-07-05 measurement), 15pp of the 22-point BFS reachability gap.

**Fix approach**: thread the version through `collect_pnpm_dep_names` via a new boolean parameter `emit_versioned: bool`. When `emit_versioned=true` (called from the v9 `snapshots:` path only), push `format!("{canon_name} {canon_ver}")` — the disambiguation-key form already indexed at `scan_fs/mod.rs:519-525` (extended for npm per issue #262 + milestone-087 cargo precedent). When `emit_versioned=false` (v6/v7 inline path), preserve pre-164 bare-name emission (User Story 2 byte-identity guard).

**Empirical target** (podman-desktop, live upstream): multi-version orphans 435 → ≤30; BFS reachability 77.4% → ≥93%. All milestone-163 invariants (SC-002/SC-004) preserved.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–163; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde_yaml` (already used pervasively in the pnpm-lock parser), `tracing`, `anyhow`. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan (matches every milestone since 002).
**Testing**: `cargo +stable test --workspace --no-fail-fast`. 8+ new unit tests (SC-007) + 1 new integration test (SC-008) + 1 optional audit test (SC-010).
**Target Platform**: All mikebom-supported hosts (Linux, macOS, Windows). No platform-specific behavior.
**Project Type**: Rust CLI (mikebom-cli) — single-crate scope; only `pnpm_lock.rs` and existing test helpers touched.
**Performance Goals**: Zero degradation. The change is a single-line push-form change inside an existing hot loop — no new allocations, no new syscalls.
**Constraints**: Constitution Principle IX (Accuracy — wrong-version edges are unacceptable), Principle V (standards-native precedence — no new `mikebom:*` annotations; reuse existing `name_to_purl` infrastructure), Principle VIII (Completeness — no silent drops on malformed keys per FR-008).
**Scale/Scope**: Live podman-desktop = 27058-line pnpm-lock.yaml with ~2668 packages and 2668 snapshot entries. Post-fix expected to correctly resolve ~435 previously-mis-targeted multi-version edges on this fixture.

## Constitution Check

**GATE**: Pass before Phase 0 research. Re-check after Phase 1 design.

Constitution v1.5.0 principles evaluated against milestone 164 scope:

- **I. Pure Rust, Zero C**: PASS — Rust stable only, no new crates, no FFI.
- **II. Deterministic Scan Output**: PASS — same input produces same output. The fix is deterministic single-pass parse.
- **III. Attestation-First**: N/A — no attestation code touched.
- **IV. No `.unwrap()` in Production**: PASS — the two-line diff at `pnpm_lock.rs:80-84` uses existing `let Some(...) = ... else { continue }` pattern; no new unwrap.
- **V. Specification Compliance (standards-native precedence)**: PASS — reuses existing `name_to_purl` disambiguation-key mechanism at `scan_fs/mod.rs:519-525`. No new `mikebom:*` annotations. FR-007 explicitly documents the reuse.
- **VI. Three-Crate Architecture (workspace layout)**: PASS — only `mikebom-cli` touched.
- **VII. eBPF-Only Observation**: N/A — user-space code path.
- **VIII. Completeness — Never Silently Drop**: PASS — FR-008 mandates a `tracing::warn!` fallback for malformed peer-dep-suffixed keys. Fall-back to bare-name form matches pre-164 behavior — no data loss.
- **IX. Accuracy — No Fake Versions**: PASS — the whole point of this milestone. Post-fix, edge targets match lockfile-declared versions exactly.
- **X. Transparency — Explicit Signals**: PASS — FR-009 mandates the info-level summary log (`multi_version_disambiguated_count`, `malformed_key_warn_count`). Grep-friendly per convention.
- **XI. Every Scan Produces an SBOM**: PASS — no scan-termination paths added.
- **XII. Ecosystem Coverage**: PASS — extends pnpm v9 support; doesn't remove coverage of any ecosystem.

**Strict Boundaries** (v1.5.0):

- §1 (deterministic PURL): PASS — emitted PURLs unchanged (FR-005: base version only, peer-dep suffix stripped).
- §2 (workspace layout): PASS.
- §3 (constitution amendment process): N/A.
- §4 (single source of truth): PASS — reuses `name_to_purl` as the single truth for edge resolution.
- §5 (no duplicate file-tier components): N/A — no file-tier code path touched.

**Verdict**: All principles + boundaries clear. No violations, no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/164-pnpm-multi-version-edges/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (empty stub — no new external contracts)
├── checklists/
│   └── requirements.md  # /speckit.specify output
└── tasks.md             # /speckit.tasks output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── npm/
│               └── pnpm_lock.rs    # ← EDITED (T003, T005): thread version through
│                                   #   `collect_pnpm_dep_names` via `emit_versioned` param.
│                                   #   FR-009 log emission.
└── tests/
    ├── pnpm_multi_version.rs        # ← NEW (T012): SC-008 integration test — synthesized
    │                                #   pnpm-lock v9 fixture with 2 versions of same pkg.
    └── pnpm_multi_version_audit.rs  # ← NEW (T017 optional): SC-010 opt-in real-testbed audit
                                     #   gated behind MIKEBOM_PNPM_MULTIVER_AUDIT=1.
```

**Structure Decision**: Single-crate scope. Only `mikebom-cli` touched. Two files edited (`pnpm_lock.rs` implementation + tests), two integration test files added. No new modules, no restructuring. This is the smallest possible surface for a bug fix of this scope — matches milestone-087's cargo-fix footprint precedent.

## Complexity Tracking

No entries required. All Constitution gates pass without justification. This is a straightforward disambiguation-key emission fix using infrastructure already validated by milestone 087.
