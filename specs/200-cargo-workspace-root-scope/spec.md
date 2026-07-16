# Feature Specification: Cargo Workspace-Root [package] Runtime Classification

**Feature Branch**: `200-cargo-workspace-root-scope`
**Created**: 2026-07-16
**Status**: Draft
**Input**: User description: "585" (GitHub issue #585 — cargo: workspace-root [package] misclassified as LifecycleScope::Development → CDX scope: excluded)

## Overview

The cargo reader misclassifies workspace-root `[package]` entries as `LifecycleScope::Development` — which serializes as CycloneDX `scope: "excluded"` and de-prioritizes the entry during m127 root-selection heuristics. Root cause is a BFS-seed gap: the prod-set BFS closure is seeded only from `[dependencies]` / `[build-dependencies]` table contents across all workspace `Cargo.toml`s. The workspace-root `[package].name` is never inserted because the root package doesn't appear inside anyone's `[dependencies]` table — it IS the workspace, not a dep of another crate. Missing seed → not in prod_set → falls through to the `Development` fallback branch.

Observed against `github.com/kusari-sandbox/test-vaultwarden`: `vaultwarden@1.0.0` (the actual application) got tagged `lifecycle-scope: development` / CDX `scope: excluded` and was demoted to a `type: library` entry in `components[]`. The root-selector then picked `macros@0.1.0` (a proc-macro helper crate) as `metadata.component` via `ecosystem-priority` heuristic, because the correctly-Runtime-tagged `macros` outranks the incorrectly-Development-tagged `vaultwarden` in the ecosystem-priority tie-break.

Downstream impact class: any cargo workspace whose root `[package]` isn't self-referenced from a member's `[dependencies]` — a very common pattern for **application** crates (as opposed to library-workspaces where members reference each other bidirectionally).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Workspace-root [package] carries the correct runtime lifecycle (Priority: P1)

An operator scans a cargo workspace where the root `Cargo.toml` declares `[package] name = "foo"` and also declares `[workspace] members = ["helper"]`. The `helper` sub-crate is a proc-macro helper referenced from `foo`'s `[dependencies]` (`helper = { path = "helper" }`). Post-scan, the emitted SBOM carries `foo` with CycloneDX `scope: null` and `mikebom:lifecycle-scope: "runtime"` — matching operator intent that `foo` IS the deliverable, not build-plumbing for it.

**Why this priority**: Wrong-scope emission cascades into every downstream consumer: CVE scanners filter on scope=excluded and skip the actual application; VEX-generation policies assume excluded scope means "not shipped"; the m127 root-selector de-prioritizes excluded-scope candidates. A single misclassification produces incorrect SBOMs at every consumption layer. Closes #585.

**Independent Test**: Fixture with a workspace-root `[package]` + one member sub-crate (proc-macro or library). Scan → assert the root component has `mikebom:lifecycle-scope: "runtime"` and CDX `scope: null` (not `"excluded"`).

**Acceptance Scenarios**:

1. **Given** a cargo workspace with root `Cargo.toml` declaring `[package] name = "app"` + `[workspace] members = ["helper"]` and root's `[dependencies]` containing `helper = { path = "helper" }`,
   **When** mikebom scans the workspace with `--offline` and emits CycloneDX,
   **Then** the `pkg:cargo/app@<ver>` component carries `mikebom:lifecycle-scope: "runtime"` (or the annotation is absent per the m179 default-Runtime-omitted convention) AND `scope: null` in the emitted CDX (not `"excluded"`).

2. **Given** the same fixture as scenario 1,
   **When** mikebom scans without `--root-name` override,
   **Then** the m127 root-selector picks `app` (not `helper`) as `metadata.component`, and the resulting SBOM has `metadata.component.name == "app"` with `type: "application"` and `scope: null`.

3. **Given** a real-world reproducer (`github.com/kusari-sandbox/test-vaultwarden`),
   **When** mikebom scans post-fix with no operator overrides,
   **Then** `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` (not `pkg:cargo/macros@0.1.0`), AND no `pkg:cargo/vaultwarden` entry appears in `components[]` with `scope: "excluded"`.

---

### User Story 2 - Non-root packages retain their existing dev/build/runtime classification (Priority: P1)

The fix MUST NOT alter the classification of any non-root cargo entry. Pre-existing dev-deps (criterion, proptest, tokio-test) remain `Development`; pre-existing build-deps (build-scripts and their transitive closures) remain `Build`; pre-existing runtime deps remain `Runtime`. Only the workspace-root `[package]` line-item changes classification.

**Why this priority**: Regression risk. The fix touches the shared BFS-seed input; if it accidentally seeds MORE than just workspace-root names, existing runtime/dev/build partitioning drifts and 200+ cargo goldens/tests break. Independent test needed to confirm scoping.

**Independent Test**: Existing cargo integration test suite passes byte-identically for the SBOM shape EXCEPT for the specific `mikebom:lifecycle-scope` and `scope` fields on workspace-root [package] components. Grep on `mikebom-cli/tests/fixtures/` post-fix reveals only the intended narrow diff.

**Acceptance Scenarios**:

1. **Given** the m083 cargo audit fixture at `mikebom-cli/tests/fixtures/transitive_parity/cargo/`,
   **When** mikebom scans it post-fix,
   **Then** every existing `mikebom:lifecycle-scope` annotation on `pkg:cargo/<name>@<ver>` components remains unchanged (verified by re-running the existing `transitive_parity_cargo.rs` integration test).

2. **Given** a cargo project with a `[dev-dependencies]` table containing `criterion = "0.5"`,
   **When** mikebom scans it,
   **Then** `pkg:cargo/criterion@0.5.x` continues to carry `mikebom:lifecycle-scope: "development"` and CDX `scope: "excluded"` (unchanged behavior).

3. **Given** the m088 cargo procmacro-edges fixture,
   **When** mikebom scans post-fix,
   **Then** proc-macro deps like `syn` and `quote` (when reached only through a `[build-dependencies]` transitive path) continue to carry `mikebom:lifecycle-scope: "build"` (unchanged behavior).

---

### Edge Cases

- **Cargo virtual workspace** (root `Cargo.toml` has `[workspace]` but no `[package]`): no root package exists to classify — fix MUST no-op cleanly, no synthetic Runtime seeding of unrelated crates.
- **Multi-workspace scan** (rootfs contains 3 unrelated Cargo workspaces): each workspace's root is Runtime for ITS OWN workspace only — cross-workspace seeding MUST NOT happen (the m083-era 3-workspace fixture is the regression guard).
- **Workspace root already reachable from a member's `[dependencies]`** (bidirectional workspace pattern — root depends on helper, helper depends on root's exported types): pre-fix, root was already in prod_set → already Runtime. Post-fix, the seed is additive, so root stays Runtime. No behavioral change for this case.
- **Workspace root that is a proc-macro or build-script** (`proc-macro = true` on the root): the fact that the root is a proc-macro doesn't change that it IS the workspace deliverable. Runtime classification is correct.
- **Excluded-path scan** (root Cargo.toml excluded via `--exclude-path`): if the root's Cargo.toml wasn't parsed, no seed insertion happens, and no root entry appears in the emitted SBOM. No behavioral change vs pre-fix.
- **Manifest parse failure** (root Cargo.toml malformed / unreadable): existing warn-and-skip behavior at `parse_cargo_toml` → no seed insertion, root falls through to Development. Documented as an expected degraded-mode outcome (matches existing behavior for any parse failure).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The cargo reader's prod-set BFS seed MUST include every workspace-member `[package].name` extracted from every parseable workspace `Cargo.toml` in the scan scope. The seed is additive to the existing `[dependencies]` / `[build-dependencies]` seeds — never subtractive.
- **FR-002**: Post-fix, every root-manifest `[package]` component MUST carry `LifecycleScope::Runtime` when the underlying dep set is Runtime (i.e., no user override to Dev/Build). This applies whether or not the root is referenced by any member's `[dependencies]` table, and whether or not a `[workspace]` block accompanies the `[package]` block — the fix seeds `[package].name` into the prod-set BFS unconditionally, so single-crate projects (which have `[package]` without `[workspace]`) get the same correction as multi-crate workspace roots.
- **FR-003**: Non-root cargo components (transitive deps, dev-deps, build-deps, workspace members that are NOT the root [package]) MUST retain their pre-fix `lifecycle_scope` classification. The BFS-seed change MUST NOT reclassify any entry that was already Runtime, Build, Development, or Optional.
- **FR-004**: The fix MUST NOT alter behavior for cargo virtual workspaces (workspaces with no root `[package]` block, only `[workspace]`). No synthetic seeding, no phantom Runtime tagging.
- **FR-005**: In multi-workspace scans (rootfs contains N unrelated Cargo workspaces), each workspace's root MUST classify against ITS OWN workspace's Cargo.toml sections only. Cross-workspace seeding is forbidden (the workspace-N root MUST NOT be seeded into workspace-M's prod_set).
- **FR-006**: A regression test fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` (or an equivalent path per m083's convention) MUST exist, containing a workspace-root `[package]` + one member sub-crate. Integration test MUST assert (a) `pkg:cargo/root@ver` component has `mikebom:lifecycle-scope: "runtime"` and CDX `scope: null`, (b) `metadata.component.purl` is the root's PURL when no `--root-name` override is passed.
- **FR-007**: `./scripts/pre-pr.sh` MUST continue to pass green post-fix. The m083 cargo audit + m088 cargo procmacro-edges + every existing cargo golden test MUST remain byte-identical for their pre-existing entries (i.e., the fix diff on existing goldens is bounded to workspace-root entries only, if any).

### Key Entities

- **Workspace-root `[package]`**: the top-level `[package]` block in a cargo workspace's root `Cargo.toml`, present when the workspace root is itself a distributable crate (application or library) — as distinct from a virtual workspace which has only `[workspace]`.
- **`prod_set` BFS closure**: the set of `(name, version)` tuples reachable via BFS from `[dependencies]`-declared direct deps, walked through `Cargo.lock`'s per-package `dependencies = [...]` field. Determines whether a `Cargo.lock` `[[package]]` entry is tagged `Runtime` (in set) or falls through to `Build` / `Development` (not in set).
- **`CargoTomlSections.prod_deps`**: the accumulator populated by `parse_cargo_toml` for every workspace Cargo.toml — the seed input to the `prod_set` BFS. Currently contains only `[dependencies]`-table keys; post-fix ALSO contains workspace-member `[package].name` values.
- **`LifecycleScope::Development` misclassification**: the failure mode this milestone fixes — workspace-root [package] falls through the m052 classifier's fallback cascade to Development, which serializes as CDX `scope: "excluded"` and cascades into m127 root-selector de-prioritization.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For `github.com/kusari-sandbox/test-vaultwarden` scanned with `mikebom --offline sbom scan --path <repo> --format cyclonedx-json --output <path> --no-deep-hash` (no operator overrides), the `vaultwarden` component MUST carry CDX `scope: null` (not `"excluded"`) — regardless of whether the m127 root-selector picks it as `metadata.component`. **Scope note**: this milestone fixes CLASSIFICATION only; a follow-up milestone will address the m127 tie-break rule that determines which workspace-root wins root election when multiple `is_workspace_root=true` components exist. Even post-m200, macros may continue to win the root-election tie-break due to a separate `is_workspace_root` annotation-propagation bug that will be filed as its own issue.
- **SC-002**: For the same scan, `jq '.components[] | select(.name == "vaultwarden") | .scope'` MUST return `null` (or the field absent). Pre-fix returned `"excluded"`.
- **SC-003**: For the same scan, `jq '[.components[] | select(.scope == "excluded")] | length'` decreases by at least 1 vs pre-fix (the vaultwarden entry no longer contributes to the excluded count).
- **SC-004**: Every existing cargo integration test in `mikebom-cli/tests/` passes without modification — no test-side assertion updates required, no cargo goldens require regen for non-root entries.
- **SC-005**: The new regression fixture + integration test (FR-006) fails cleanly against pre-fix code and passes against post-fix code (proves the fix is load-bearing).
- **SC-006**: `./scripts/pre-pr.sh` wall-clock delta ≤ 5 seconds vs pre-m200 baseline (matches m195-m199 SC threshold).
- **SC-007**: Post-merge, the `#585` GitHub issue closes automatically via `Closes #585` in the PR body.

## Assumptions

- **BFS seed order is deterministic**: `HashSet` iteration in Rust doesn't guarantee order, but the BFS closure's OUTPUT is order-independent (a set-membership check). Adding to the seed set produces the same closure regardless of insertion order — no golden byte-diff risk from ordering.
- **Existing workspace-member classification is already correct**: workspace members reachable from `[dependencies]` (like `helper` referenced from `app`'s deps) are already correctly Runtime pre-fix. The fix ONLY affects roots that aren't self-referenced from their own members' deps tables.
- **Non-cargo ecosystems unchanged**: npm / pip / maven / etc. readers have their own lifecycle classification code paths. This milestone is cargo-scoped.
- **No new Cargo dependencies**: the fix is a 2-line addition to `parse_cargo_toml` — reading the `[package].name` key alongside the existing `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` sections. No crates added.
- **Constitution Principle V compliance**: this milestone does NOT introduce any new `mikebom:*` annotations. It corrects a value in an EXISTING `mikebom:lifecycle-scope` annotation (Runtime instead of Development), which maps to existing CDX / SPDX native constructs per m052's audit. No Principle-V audit needed.
- **Regression goldens scope**: expected 0 pre-existing cargo goldens require regen (the m083 audit fixture is a virtual workspace with no root [package]; the m088 procmacro-edges fixture is a single-crate project with no `[workspace]`). Verified during implement phase via grep — if any goldens contain a workspace-root [package] previously misclassified as excluded, they regen. Following the m199 empirical-verification lesson, this assumption is re-verified at implement-time before final scope estimation.
