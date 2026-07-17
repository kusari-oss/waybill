# Feature Specification: Root-Selector Workspace-Root Disambiguation in Multi-Ecosystem Scans

**Feature Branch**: `201-root-selector-workspace-root-fix`
**Created**: 2026-07-17
**Status**: Draft
**Input**: User description: "m201" (GitHub issue #587 — m127 root-selector `is_workspace_root` propagation quirk when workspace has cargo + npm at same manifest root)

## Overview

When a cargo workspace's root Cargo.toml declares `[package]` alongside `[workspace] members = [...]`, both the ROOT crate and every workspace-MEMBER crate get incorrectly stamped `mikebom:is-workspace-root = true` — because they share the SAME workspace `Cargo.lock` path in `evidence.source_file_paths` (a side effect of the m064 "augment-in-place" emission pattern). The m127 root-selector's RepoRoot ladder branch fires only when EXACTLY ONE main-module is `is_workspace_root = true`; with 2+, the ladder falls through to `ecosystem-priority`, whose tie-break picks alphabetically-first candidates. In test-vaultwarden, that means `macros@0.1.0` (a proc-macro helper) wins over `vaultwarden@1.0.0` (the actual application).

**User-observable symptom**: `metadata.component` is the wrong crate. Downstream CVE-consumer pipelines that key on `metadata.component.purl` receive an incorrect SBOM identity.

**m200 relationship**: m200 (#586) already fixed the classification side of the same underlying bug — `vaultwarden` is now correctly Runtime (`scope: null`) instead of Development (`scope: "excluded"`). m201 completes the story by fixing the ROOT ELECTION side, so `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` without operator overrides.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Workspace root wins root election even with sibling member main-modules (Priority: P1)

An operator scans a cargo workspace where the root `Cargo.toml` declares `[package] name = "app"` alongside `[workspace] members = ["helper"]`. Both `app` (workspace root) and `helper` (workspace member) end up as main-module candidates. The operator expects `metadata.component` to be `pkg:cargo/app@<ver>` — the actual deliverable — not `pkg:cargo/helper@<ver>` (a workspace helper crate). This holds regardless of alphabetical ordering of the crate names, and regardless of whether other ecosystems (npm subdirectory, etc.) also contribute main-module candidates.

**Why this priority**: This is the user-visible bug from #585/#586 that m200 did NOT fully close. Without m201, every multi-crate cargo workspace still needs `--root-name` / `--root-purl-type` operator overrides to produce a correct SBOM. Consumer pipelines keying on `metadata.component.purl` receive incorrect identity. Closes #587.

**Independent Test**: Extend the m200 fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` (or create a new fixture) so it also has an npm sub-project (e.g., `helper-npm/package.json` with `name: "helper-npm"`) — producing 3 main-module candidates (cargo-root `app`, cargo-member `helper`, npm-nested `helper-npm`). Scan → assert `metadata.component.purl == "pkg:cargo/app@0.1.0"` via the RepoRoot heuristic (heuristic name `"repo-root"`, NOT `"ecosystem-priority"`).

**Acceptance Scenarios**:

1. **Given** a cargo workspace with root `[package] name = "app"` + `[workspace] members = ["helper"]` + a nested npm project at `sub/package.json` with `name: "sub"`,
   **When** mikebom scans the workspace with `--offline` and no `--root-name` override,
   **Then** the emitted SBOM has `metadata.component.name == "app"`, `metadata.component.purl` starts with `"pkg:cargo/app@"`, AND the scan log's root-election summary reports `heuristic = "repo-root"` (not `"ecosystem-priority"`).

2. **Given** the real-world reproducer at `github.com/kusari-sandbox/test-vaultwarden`,
   **When** mikebom scans with no operator overrides post-fix,
   **Then** `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` (was `pkg:cargo/macros@0.1.0` pre-m201 / post-m200).

3. **Given** the same scan as scenario 2,
   **When** parsed for the losers list,
   **Then** `pkg:cargo/macros@0.1.0` appears in the `losers[]` array (not the winner), alongside `pkg:npm/scenarios@1.0.0`.

---

### User Story 2 - Non-vaultwarden-shape scans retain their existing root-election behavior (Priority: P1)

The fix MUST NOT alter root election for scans that already produce correct results. Existing single-crate scans, virtual-workspace scans, monorepos where the root election was previously correct, and every existing integration test's root-election outcome MUST remain byte-identical.

**Why this priority**: Regression risk. The m127 root-selector is a critical single-point-of-truth for CDX `metadata.component` identity; broken behavior on ANY existing fixture is a shipping blocker.

**Independent Test**: Existing cargo integration tests (`transitive_parity_cargo`, `optional_dep_classification`, `produces_binaries_cargo`, `scan_cargo`, `cargo_workspace_root_lifecycle_m200`) all pass without modification. Existing golden JSONs' `metadata.component.purl` values are byte-identical pre/post-m201.

**Acceptance Scenarios**:

1. **Given** any existing cargo fixture in `mikebom-cli/tests/fixtures/` that DID have correct root election pre-m201,
   **When** mikebom scans it post-m201,
   **Then** `metadata.component.purl` is byte-identical to pre-m201 output.

2. **Given** a virtual workspace (root `Cargo.toml` has `[workspace]` but no `[package]`),
   **When** mikebom scans it,
   **Then** root election proceeds via the existing ladder (LCP, synthetic placeholder, etc.) unchanged.

3. **Given** a single-crate cargo project (no `[workspace]` block),
   **When** mikebom scans it,
   **Then** root election picks that single crate as `metadata.component` (unchanged behavior — the RepoRoot ladder branch fires as before for the single-candidate case).

---

### Edge Cases

- **Cargo virtual workspace**: no `[package]` block at root — the fix's disambiguation logic MUST NOT synthesize a workspace-root candidate. Existing behavior (LCP heuristic or synthetic placeholder) proceeds unchanged.
- **Workspace with multiple root-level `[package]` blocks nested in sibling paths**: e.g., a rootfs containing two INDEPENDENT cargo projects at `project-a/Cargo.toml` and `project-b/Cargo.toml`. Neither Cargo.toml is at `rootfs/`. Each project independently classifies its own workspace root — no cross-workspace root election tie-break beyond what LCP already handles.
- **Multi-ecosystem scan where the cargo workspace root COEXISTS with an npm project at the same rootfs**: e.g., `<rootfs>/Cargo.toml` + `<rootfs>/package.json` (both at repo root). Both should be `is_workspace_root = true`. The fix disambiguates via a follow-up tie-break (e.g., ecosystem-priority) or explicitly acknowledges this case as ambiguous and preserves ecosystem-priority behavior for it.
- **Workspace root that's a proc-macro** (`proc-macro = true` on the root `[package]`): unusual but legal. The fix MUST NOT de-prioritize proc-macro crates from root-election — being a proc-macro doesn't disqualify the workspace root from being the deliverable.
- **Workspace root with no `src/main.rs` or `[[bin]]` declaration**: a library-only workspace root. The fix MUST still pick it as the workspace root (library workspaces are common — e.g., serde, tokio).
- **Nested cargo workspaces**: `rootfs/Cargo.toml` has `[workspace] members = ["sub"]` and `rootfs/sub/Cargo.toml` also has `[workspace] members = ["deeper"]`. Both `rootfs` and `rootfs/sub` are workspace roots for their respective scopes. The fix defers to the OUTER workspace root (whichever crate's Cargo.toml is at the shallowest rootfs-relative path).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `mikebom:is-workspace-root` annotation stamping (currently at `mikebom-cli/src/scan_fs/mod.rs:944-947`) MUST distinguish workspace-ROOT [package] entries from workspace-MEMBER [package] entries when both share the same `evidence.source_file_paths[0]` (typically the workspace `Cargo.lock` path). Post-fix, at most ONE cargo main-module per scan carries `is_workspace_root = true`.
- **FR-002**: For a scan where exactly ONE cargo main-module is `is_workspace_root = true`, the m127 root-selector's RepoRoot ladder branch (`workspace_root_modules.len() == 1` at `root_selector.rs:243-250`) MUST fire and the workspace-root cargo main-module MUST be `metadata.component`. Heuristic name reported in the scan log is `"repo-root"` (not `"ecosystem-priority"`).
- **FR-003**: Non-cargo main-modules whose manifest path is at rootfs (e.g., an npm `package.json` at `rootfs/package.json`) MUST retain their existing `is_workspace_root = true` behavior. Cross-ecosystem cases where multiple ecosystems have their manifest at rootfs continue to fall through the RepoRoot branch (workspace_root_modules > 1) into the existing ecosystem-priority ladder — same as pre-m201.
- **FR-004**: Existing root-election behavior for every non-vaultwarden-shape scan (single-crate cargo, virtual cargo workspace, npm-only projects, python-only projects, mixed monorepos where root election was previously correct) MUST be preserved byte-identically. No existing integration test's `metadata.component.purl` may change.
- **FR-005**: A new regression fixture (or an extension of the m200 fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/`) MUST reproduce the multi-main-module case from #587 and assert `metadata.component.purl` post-m201 is the workspace-root cargo crate, with the scan log reporting heuristic `"repo-root"`.
- **FR-006**: `./scripts/pre-pr.sh` MUST continue to pass green post-fix. All existing cargo goldens (including public-corpus `rust-ripgrep`, m083 audit, m088 procmacro-edges, m200 root_package_lifecycle) MUST hold byte-identically for their pre-existing entries.
- **FR-007**: The fix MUST NOT introduce any new `mikebom:*` annotation. It MAY modify the semantics of the existing `mikebom:is-workspace-root` annotation (which is internal-emission-only per `is_internal_emission_key` at `root_selector.rs:437-439` — it doesn't appear in emitted SBOMs). Alternatively, the fix MAY route a new signal through an existing internal-only annotation.

### Key Entities

- **`mikebom:is-workspace-root` (internal-only annotation)**: boolean flag stamped by `scan_fs/mod.rs:944-947` on every component tagged `mikebom:component-role: main-module`. Consumed by the m127 root-selector's RepoRoot ladder branch to disambiguate the "which main-module IS the workspace root" question. Filtered out at CDX/SPDX emission (per `is_internal_emission_key`) so it's a purely internal signal.
- **`evidence.source_file_paths[0]` (per-component)**: the primary manifest-path anchor consulted by the `is_workspace_root` stamping logic. For augmented main-modules under cargo's m064 "augment-in-place" pattern, this is the shared workspace `Cargo.lock` path — the SAME string for every cargo main-module in the workspace. This shared-value collision is the source of the bug.
- **`m200 root_names` (existing accumulator)**: `HashSet<String>` populated by `parse_cargo_toml` recording every workspace-member `[package].name`. Available at cargo-reader time. m201 MAY leverage this to distinguish the workspace ROOT `[package].name` (top-level Cargo.toml) from workspace MEMBER names — pending plan-phase mechanism selection.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For `github.com/kusari-sandbox/test-vaultwarden` scanned with `mikebom --offline sbom scan --path <repo> --format cyclonedx-json --output <path> --no-deep-hash` (no operator overrides), `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` (was `pkg:cargo/macros@0.1.0` post-m200 / pre-m201).
- **SC-002**: The scan log's root-election summary for the same scan reports `heuristic = "repo-root"` (was `heuristic = "ecosystem-priority"` post-m200 / pre-m201). Confidence value increases from 0.70 (ecosystem-priority) to whatever the RepoRoot ladder emits (currently 0.90 per `RootSelectionHeuristic::RepoRoot.confidence()`).
- **SC-003**: For the same scan, `losers[]` contains `"pkg:cargo/macros@0.1.0"` and `"pkg:npm/scenarios@1.0.0"` (both non-winners of the root election).
- **SC-004**: Every existing cargo integration test in `mikebom-cli/tests/` passes without modification. No test-side assertion updates required. No existing golden's `metadata.component.purl` changes byte-identically.
- **SC-005**: The new regression fixture + integration test (FR-005) fails cleanly against pre-m201 code and passes against post-m201 code (proves the fix is load-bearing).
- **SC-006**: `./scripts/pre-pr.sh` wall-clock delta ≤ 5 seconds vs pre-m201 baseline.
- **SC-007**: Post-merge, `#587` closes automatically via `Closes #587` in the PR body.

## Assumptions

- **m200 is landed**: this milestone builds on m200's `root_names` accumulator + `is_workspace_root` stamping infrastructure. Both must be present on `main` before m201 starts.
- **The fix is scoped to `is_workspace_root` disambiguation** (or its consumer at the m127 RepoRoot ladder), NOT to the cargo m064 augment-in-place pattern. Modifying cargo m064 to emit per-crate `source_file_paths` for augmented entries would have broader cascading effects on other tests and goldens; the m201 scope prefers the narrower fix.
- **`is_workspace_root` is internal-emission-only**: since the annotation is filtered from emitted SBOMs, changes to its stamping semantics don't affect wire-format consumers. Only the internal m127 root-selector observes it.
- **Non-cargo ecosystems' `is_workspace_root` behavior is unchanged**: npm / python / maven / etc. readers emit their own main-modules with their own `evidence.source_file_paths` values (typically the ecosystem-specific manifest path). Only cargo's shared-lockfile pattern causes the collision fixed here.
- **Zero new Cargo dependencies**: the fix is Rust-only, leverages existing `HashSet<String>` machinery.
- **Constitution Principle V compliance**: no new `mikebom:*` annotations introduced. Modifying an existing internal-only annotation's stamping semantics does not require a Principle-V audit citation.
- **Regression goldens scope**: expected 0 pre-existing goldens require regen. Following the m199 empirical-verification lesson, re-verified at implement time before final scope estimation.
