# Implementation Plan: Windows-host build + run support

**Branch**: `100-windows-host-build` | **Date**: 2026-05-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/Users/mlieberman/Projects/mikebom/specs/100-windows-host-build/spec.md`

## Summary

Enable mikebom-cli to build, test, and run on Windows hosts for the cross-platform use cases (Rust/npm/Python/Ruby/Java/Go ecosystem scans + arbitrary PE/ELF/Mach-O binary inspection). Three concrete deliverables:

1. **Audit + close any compile-on-Windows gaps**: the 94 existing `#[cfg(unix)]` gates are all `claimed_inodes`-plumbing for the path-claim dedup or POSIX file-mode/inode access — none represent functional behavior that Windows requires. Audit confirms each gate is appropriately isolated and Windows builds produce a no-op-but-correct claim-dedup path.
2. **Implement forward-slash path normalization** per the Clarifications decision. Single chokepoint at the `ResolvedComponent.evidence.source_file_paths` field population in `scan_fs/mod.rs` + 3 generator-side emission sites (CDX `evidence.occurrences[].location`, SPDX 2.3 + SPDX 3 annotation emission). Add a `normalize_sbom_path(&Path) -> String` helper that replaces backslash with forward-slash on Windows and is a no-op on Unix.
3. **CI + release pipeline**: add `lint-and-test-windows` job to `.github/workflows/ci.yml` mirroring the existing `lint-and-test-macos` shape; add `build-windows-x86_64` job to `release.yml` producing a `.zip` artifact. Update the `release` aggregation job's `needs:` list to include the new Windows build.

Net diff forecast: ~200 lines of Rust (path-normalization helper + ~88 call-site updates via a typed wrapper, OR a sed-style replacement; implementer's choice) + ~50 lines of CI YAML + ~50 lines of release YAML + README/docs refresh. Zero new Cargo dependencies. ~3-5 hours single-developer effort.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–099; no nightly required for Windows-host work).
**Primary Dependencies**: Existing only. Workspace deps (`object`, `serde`, `clap`, `tokio`, `reqwest` with `rustls-tls`, `chrono`, `regex`, `sha2`, `git2`-free since we shell out to git) all support Windows. Verified at planning-time via `cargo tree --target x86_64-pc-windows-msvc -p mikebom` (run during T002).
**Storage**: N/A — pure host-portability work; no state additions.
**Testing**: `cargo +stable test --workspace` on `windows-latest` GitHub runners. POSIX-specific tests gated `#[cfg(unix)]` or graceful-skip (no `/bin/ls`, etc.).
**Target Platform**: `x86_64-pc-windows-msvc` for v1. ARM64 Windows (`aarch64-pc-windows-msvc`) deferred per Out-of-Scope.
**Project Type**: Rust CLI workspace (`mikebom-cli` binary + `mikebom-common` lib + `xtask` build helper).
**Performance Goals**: Equivalent to Linux/macOS hosts — path-normalization is an O(string-length) `replace('\\', '/')` per emitted path; negligible.
**Constraints**: Zero new Cargo deps (FR-005-derived → SC-007). Zero schema changes (SC-008). Forward-slash normalization applies to *all* emitted SBOM JSON path strings on every host (preserves cross-host byte-identity goldens).
**Scale/Scope**: Mid-size milestone. 3 deliverable streams: code-side audit + path normalization (~200 lines), CI YAML (~50 lines), release YAML + README (~75 lines). Pre-PR-gate-Windows-port is explicitly out of scope.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Rationale |
|-----------|--------|-----------|
| **I. Pure Rust, Zero C** | ✅ PASS | Pure-Rust changes; no new deps. The MSVC target uses Microsoft's linker (no new C deps in our code). |
| **II. eBPF-Only Observation** | ✅ N/A | This milestone is host-portability, not discovery. The `ebpf-tracing` feature flag stays Linux-only by design. |
| **IV. Test Discipline** | ✅ PASS | New Windows CI lane runs the same clippy + test gate as Linux/macOS. Per FR-002 + FR-003. |
| **V. Specification Compliance** | ✅ N/A | No SBOM-schema changes. Path normalization changes the *value* of `location` strings (from backslash to forward-slash) but the *shape* of the JSON is unchanged. Cross-host byte-identity goldens improve (regenerate once, valid on all hosts). |
| **X. Transparency** | ✅ PASS | The path-normalization rule is documented in the Clarifications section + propagated into FR-004 + the Edge Cases section. Operators consuming SBOMs see consistent path-format across host OSes. |
| **XII. External Data Source Enrichment** | ✅ N/A | No new external data sources. |

**No CRITICAL violations.** Path normalization is internal serialization shape preservation, not a schema change.

## Project Structure

### Documentation (this feature)

```text
specs/100-windows-host-build/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
├── checklists/
│   └── requirements.md  # Already exists
├── spec.md              # Already exists (+ Clarifications)
└── tasks.md             # Phase 2 output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/
├── sbom_path.rs            # NEW — `normalize_sbom_path(&Path) -> String` helper.
└── mod.rs                  # MODIFY — chokepoint: normalize when populating
                             #          `ResolvedComponent.evidence.source_file_paths`
                             #          + `ResolvedComponent.source_path` from `PackageDbEntry`.

mikebom-cli/src/generate/
├── cyclonedx/evidence.rs   # MODIFY — `evidence.occurrences[].location` emission
                             #          calls normalize helper (defense-in-depth even
                             #          if chokepoint above already normalized).
├── spdx/annotations.rs     # MODIFY — same defense-in-depth for SPDX 2.3 emission.
└── spdx/v3_annotations.rs  # MODIFY — same for SPDX 3.

.github/workflows/
├── ci.yml                  # MODIFY — add `lint-and-test-windows` job after macOS lane.
└── release.yml             # MODIFY — add `build-windows-x86_64` job; update `release`
                             #          job's `needs:` list to include new build.

README.md                   # MODIFY — Windows install + usage section.
```

**Structure Decision**:
- **Path normalization at the deduplicator boundary**: single chokepoint at the `ResolvedComponent`-population sites in `scan_fs/mod.rs`. The 88 `to_string_lossy()` call sites in package_db readers continue to populate `PackageDbEntry.source_path` with native-OS strings; the normalization runs once when those entries become `ResolvedComponent`s. Defensive normalization at the 3 JSON-emission sites guards against future code paths that bypass the deduplicator.
- **CI parallel to existing lanes**: Windows lane runs alongside Linux + macOS in parallel; no serialization. Total CI wall-clock unchanged.
- **Release zip vs tarball**: Windows convention uses `.zip` (PowerShell `Compress-Archive` or Git Bash `zip`). The SHA256SUMS aggregation in the `release` job handles both formats transparently.

## Complexity Tracking

No constitution violations. Table empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
