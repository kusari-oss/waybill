# Implementation Plan: npm workspace-peer phantom empty-version edges (fix + regression guard)

**Branch**: `163-npm-phantom-edges` | **Date**: 2026-07-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/163-npm-phantom-edges/spec.md`

## Summary

Milestone 158's audit surfaced that mikebom silently emits 159 components with empty-version PURLs (`pkg:npm/name@` — no version segment) + 902 phantom edges (14.5% of the graph) targeting those PURLs on `kusari-sandbox/test-podman-desktop`. BFS reachability caps at only 24.6% (698 of 2835 npm components). The real resolved versions exist as components in the same SBOM — the workspace-peer readers just aren't cross-resolving.

**Failure mechanism**: mikebom's npm reader walks the filesystem for every directory with a `package.json`. Each becomes a "project root" processed through 3 tiers (A = lockfile, B = `node_modules/`, C = `parse_root_package_json` design-tier fallback). For the workspace ROOT, Tier A wins → real resolved versions emitted. For each workspace PEER (e.g. `packages/docs/`, `apps/renderer/`), there's no local lockfile, no local `node_modules/` (hoisted to root), so Tier C fires → `parse_root_package_json` emits `pkg:npm/<name>@` (empty version) for every declared dep. Consumer edges from the peer's component target these phantom PURLs, resolving to nothing.

**Technical approach** (per Q1+Q2 unified disposition + FR-001/FR-003/FR-004):

1. **Build a cross-workspace resolution index** after Tier A completes for every project root. Map: `HashMap<String, String>` from `name → concrete-version`. When the root's `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock v1` / `bun.lock` resolves `@docusaurus/core@3.10.1`, that mapping enters the index.
2. **Reshape Tier C for workspace peers**: when `parse_root_package_json` fires for a peer's `package.json`, check every declared dep against the cross-workspace index BEFORE emitting a phantom component:
   - **Cross-resolution HIT** (name in index) → don't emit a design-tier phantom; instead add the dep name to a per-source `resolved_workspace_peer_deps` sidecar so the graph resolver wires the edge to the real `pkg:npm/<name>@<version>` component.
   - **Cross-resolution MISS** (name NOT in index) → per Q1: suppress the edge + emit `mikebom:unresolved-declared-dep = "<name>"` on the source workspace-peer component.
3. **FR-003 closest-ancestor semantics** (nested node_modules): if a workspace peer has its own `packages/foo/node_modules/@docusaurus/core@3.9.0` (different from root's 3.10.1), the peer's edge targets 3.9.0 (nested wins). Implementation: consult a per-peer version-override map derived from the peer's own node_modules walk BEFORE falling through to the root cross-workspace index.
4. **Emit `mikebom:unresolved-declared-dep` as a per-component annotation** on workspace-peer components (C115). Bare string for single unresolved dep; JSON array for multiple. Matches milestone-159 C106/C107 + milestone-162 C114 multi-value precedent.
5. **Preserve every real component** — SC-005 (2835 count preserved) enforced by NOT dropping any existing lockfile-derived entry. Only the design-tier phantom emissions change: from `pkg:npm/name@` component → to source-side annotations OR real edges.
6. **Q1+Q2 unified disposition**: single classifier function returns `Resolved { version }` OR `Unresolved`. Both "no lockfile entry" (Q1) and "range mismatch" (Q2) collapse to `Unresolved`. Q2's semver-comparison is deferred to FR-003 — when a peer says `"^4.0.0"` but only `3.10.1` is resolved, mikebom does NOT do semver matching; it treats the resolved version as authoritative (Node.js runtime would too — the lockfile is the source of truth). So Q2's "mismatch" in practice reduces to Q1's "unresolvable" only when the lockfile truly doesn't contain the name.

Q1+Q2 clarifications (spec §Clarifications) lock: unified SUPPRESS + `mikebom:unresolved-declared-dep` annotation disposition for both failure classes.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–162; no nightly required).

**Primary Dependencies**: Existing only — `mikebom_common::types::purl::{Purl, encode_purl_segment}` (already used by `npm/mod.rs::build_npm_purl`), `serde`/`serde_json` (annotation values), `tracing` (FR-009 log), `anyhow`/`thiserror` (error propagation). **Zero new Cargo dependencies.** No semver crate needed — Q2 disposition sidesteps range-comparison logic (the lockfile is authoritative).

**Storage**: N/A — cross-workspace resolution index lives in `Vec<PackageDbEntry>` scan-locally, rebuilt from Tier-A output per scan. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace --no-fail-fast` per Constitution Development Workflow. New tests in three tiers per milestone-055/091/158/160/161/162 precedent:
- Unit tests in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` (module-inline `#[cfg(test)]`) covering `parse_root_package_json` behavior change: cross-resolution hit → no phantom emission + edge routing; cross-resolution miss → source-side annotation.
- Unit tests in `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` covering the cross-workspace resolution index construction + FR-003 closest-ancestor precedence.
- Integration test at `mikebom-cli/tests/npm_phantom_edges.rs` (per SC-008) with a synthesized multi-workspace monorepo exercising the release binary end-to-end.

**Target Platform**: Linux + macOS + Windows dev hosts (per milestones 100/101). No platform-specific behavior.

**Project Type**: CLI (Rust workspace with 3 crates per Constitution Principle VI).

**Performance Goals**: O(components) index construction after Tier A — negligible overhead. Per-workspace-peer lookup is O(deps) HashMap probes. Total added scan cost on test-podman-desktop (2835 components × avg 5 deps per peer × ~10 peers): ~150 index inserts + ~500 HashMap lookups per scan = sub-millisecond.

**Constraints**: **No new Cargo dependencies**. **No `.unwrap()` in production** per Constitution Principle IV. **Standards-native precedence** per Principle V — FR-007 documents the audit (no CDX/SPDX-native "declared-but-unresolved" field as of 2026-07-05).

**Scale/Scope**: `test-podman-desktop` has 2835 npm components with 902 phantom edges (14.5% of the graph). Post-163: 0 phantom edges + 159 empty-version components collapse to source-side annotations (some become real edges via FR-001). Golden regeneration impact: 10 non-`npm` milestone-090 fixtures × 3 formats = 30 goldens byte-identical (SC-003); the `npm` fixture goldens MAY change if its `package.json` files declare cross-resolvable deps.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle-by-principle assessment

**I. Pure Rust, Zero C** — ✅ PASS. All work is user-space Rust in `mikebom-cli`. No FFI. No C dependencies added. `mikebom-ebpf` untouched.

**II. eBPF-Only Observation** — ✅ N/A. Milestone 163 does not touch discovery — it's an emission-layer correctness fix. Discovery of `package.json` files continues via the existing `candidate_project_roots` walker.

**III. Fail Closed** — ✅ PASS. Cross-resolution misses fail gracefully: no phantom emission, no fake edges, source-side annotation surfaces the miss to consumers. Runtime doesn't fall back to guessing versions.

**IV. Type-Driven Correctness** — ✅ PASS. New `CrossResolution` enum with 2 variants (`Resolved { version }` / `Unresolved`) drives the emission branch. All new annotation values are string-typed matching the milestone-159 C106/C107 shape. No `.unwrap()` in production paths.

**V. Specification Compliance** — ⚠️ GATE. Two audit checks required:
- **Native-first check**: FR-007 explicitly documents the audit — no CDX 1.6 or SPDX 3.0.1 native field for "declared-but-unresolved dep" as of 2026-07-05. The `mikebom:unresolved-declared-dep` prefix is compliant per Principle V's "parity-bridging" clause.
- **Existing-mikebom-annotation check**: no existing per-component annotation carries "peer declared a dep that couldn't be cross-resolved" semantic. Milestone-160 C109 (`mikebom:go-transitive-unresolved-reason`) is Go-specific per-module; C115 is npm-specific per-workspace-peer. Orthogonal.

**VI. Three-Crate Architecture** — ✅ PASS. All changes in `mikebom-cli`; no new crates.

**VII. Test Isolation** — ✅ PASS. New unit tests are pure logic (cross-resolution classifier + FR-003 closest-ancestor precedence). Integration test uses a synthesized multi-workspace monorepo in a tempdir — no eBPF privilege.

**VIII. Completeness** — ✅ CENTRAL. Milestone 163 directly addresses the completeness gap discovered in milestone-158's audit (BFS reachability 24.6% → ≥99%). Every previously-phantom edge either becomes a real edge to a resolved component OR gets suppressed with a transparency annotation — the SBOM never claims edges to non-existent PURLs.

**IX. Accuracy** — ✅ CENTRAL. Post-163: zero PURLs match the shape `pkg:npm/*@` (empty version segment). Zero edges to non-existent PURL targets. Consumer trust surface is explicit.

**X. Transparency** — ✅ CENTRAL. The `mikebom:unresolved-declared-dep` annotation surfaces every declaration that didn't cleanly resolve, unified across Q1 (no lockfile entry) and Q2 (range mismatch) failure classes. Consumer switch statements gain a single clear signal.

**XI. Enrichment** — ✅ N/A. Milestone 163 does not fetch new external data.

**XII. External Data Source Enrichment** — ✅ N/A. Cross-resolution reads from the already-emitted lockfile-derived entries. No new external source.

### Strict Boundary compliance

**§1 (No lockfile discovery)** — ✅ PASS. Lockfiles are used ONLY for ENRICHMENT (cross-resolving already-declared deps to concrete versions). Milestone 163 does NOT use lockfiles to add new components — existing Tier A behavior is preserved unchanged.

**§2 (No MITM proxy)** — ✅ PASS.

**§3 (No C code)** — ✅ PASS.

**§4 (No `.unwrap()` in production)** — ✅ PASS.

**§5 (No file-tier duplicates in default mode)** — ✅ N/A.

### Gate result

Constitution Check **PASSES** — no violations. All principles + boundaries compliant.

## Project Structure

### Documentation (this feature)

```text
specs/163-npm-phantom-edges/
├── plan.md              # This file
├── research.md          # Phase 0 output (R1–R6 below)
├── data-model.md        # Phase 1 output (entities: CrossResolution, resolution index, C115 annotation)
├── quickstart.md        # Phase 1 output (contributor path: build+test)
├── contracts/
│   └── annotations.md   # Phase 1 output (per-format wire shape for C115)
├── checklists/
│   └── requirements.md  # Already exists from /speckit-specify
└── tasks.md             # /speckit-tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── npm/
│   │           ├── mod.rs                    # EDIT: build cross-workspace resolution index after Tier A; pass into Tier C invocation for workspace peers
│   │           ├── walk.rs                   # EDIT: parse_root_package_json takes an optional cross-resolution index; on hit → skip design-tier phantom + accumulate resolved-deps sidecar; on miss → emit mikebom:unresolved-declared-dep on peer's main-module component
│   │           └── (other files unchanged)
│   └── parity/
│       └── extractors/
│           ├── mod.rs                        # EDIT: register C115 row
│           ├── cdx.rs                        # EDIT: cdx_anno!(c115_cdx, "mikebom:unresolved-declared-dep", component)
│           ├── spdx2.rs                      # EDIT: spdx23_anno! invocation
│           └── spdx3.rs                      # EDIT: spdx3_anno! invocation
└── tests/
    └── npm_phantom_edges.rs                  # NEW: SC-008 integration test (synthesized multi-workspace monorepo)
```

**Structure Decision**: Milestone 163 is a targeted extension of the existing `npm/mod.rs` + `npm/walk.rs` reader plus 4 parity-catalog registrations. No new source-tree directories. Smallest source-tree footprint after milestone 162.

## Complexity Tracking

*No Constitution violations. Section not applicable.*

## Phase completion status

- ✅ **Phase 0 (research)** — see `research.md` for R1–R6 resolutions.
- ✅ **Phase 1 (design & contracts)** — see `data-model.md`, `contracts/annotations.md`, `quickstart.md`.
- 🔲 **Phase 2 (task decomposition)** — deferred to `/speckit-tasks`.

## Post-design constitution re-check

Post-design re-check passes. R1 confirms C115 is semantically distinct from C109 (Go-specific per-module unresolved-reason). R2 pins the cross-workspace resolution index as a scan-local `HashMap<String, String>` (name → version) constructed once after Tier A. R3 defers Q2 range-comparison — the lockfile is authoritative; no semver logic needed.

## Notes

- Genuinely simpler than milestones 160/161 — no empirical investigation loop. The fix mechanism is fully known: cross-resolution against the top-level lockfile.
- The 902 phantom edges + 159 empty-version PURLs from milestone-158's audit are the load-bearing SC-002/SC-004 evidence.
- No fixture-cache dependency for the P1 MVP — the SC-008 integration test synthesizes a minimal multi-workspace monorepo. The SC-001 test-podman-desktop verification is opportunistic (via a new gated audit test at `mikebom-cli/tests/npm_phantom_edges_audit.rs`) but not blocking.
- Delivers milestone-158's ≥99% BFS reachability aspiration for the npm ecosystem.
- Constitutional Principles VIII (Completeness) + IX (Accuracy) + X (Transparency) all central to this milestone — this is the archetypal "no fake edges" fix.
