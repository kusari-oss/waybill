# Feature Specification: Fix Cargo Optional-Dep Over-Exclusion — Resolve Feature Activation

**Feature Branch**: `205-cargo-optional-feature-resolve`
**Created**: 2026-07-17
**Status**: Draft
**Input**: Issue #593 — cargo optional-dep classifier over-excludes feature-activated deps → silent vuln under-reporting. External reporter: @nchelluri, gist: https://gist.github.com/nchelluri/8e74c2d7d3761c74be57dcecf5bc92df.

## Background

Milestone 179 introduced the `LifecycleScope::Optional` classification. The Cargo-side classifier at `mikebom-cli/src/scan_fs/package_db/cargo.rs:1155` marks a resolved package as `Optional` (→ CDX `scope: excluded`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`) when the package's NAME appears in a workspace-wide HashSet of `optional = true` manifest declarations.

The check does not resolve **feature activation**. In Cargo semantics, an `optional = true` dep IS compiled into the artifact if any enabled feature (default feature list, `--features` flag, or an implicit-features cross-reference) activates it. Ignoring feature resolution causes actually-shipped deps — and their exclusive subtrees — to be marked `scope: excluded`, which downstream vulnerability scanners honor by pruning the component from analysis. **Silent under-reporting of vulnerabilities is the observed consequence.**

External reporter's concrete case (test-vaultwarden, alpha.63): `reqsign-aws-v4@3.0.1` gets `scope: excluded` under `mikebom:optional-derivation: cargo-optional-true`, but `opendal-service-s3@0.57.0` (which activates it via a feature) stays in scope. The subtree pruning that follows removes `quick-xml@0.40.1` from scanner output → RUSTSEC-2026-0194/0195 silently drop out of vuln reports.

m205 corrects the classifier to reflect Cargo's actual resolution.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Feature-activated optional dep stays in scope (Priority: P1)

An operator scans a Cargo workspace where a dep is declared `optional = true` but is activated by the workspace's `default` feature list (or by a downstream member's feature reference). The emitted SBOM includes the dep as `scope: runtime` (CDX) / `RUNTIME_DEPENDENCY_OF` (SPDX 2.3) — matching what `cargo build` actually compiles — NOT `scope: excluded`.

**Why this priority**: This is the failure mode described in the reporter's gist. It's the difference between a security scanner seeing a vulnerable dep versus silently dropping it. Regression versus alpha.62 behavior. Every alpha.63+ scan of any Cargo project using default-feature-activated optional deps under-reports vulnerabilities right now. P1 because the security-scanner use case is a first-class mikebom persona and this bug materially breaks it.

**Independent Test**: Construct a minimal Cargo workspace with `[dependencies] foo = { version = "*", optional = true }` + `[features] default = ["foo"]`. Scan with `mikebom sbom scan --path <ws>`. Assert `foo`'s emitted component has `scope: "runtime"` (CDX) / no `OPTIONAL_DEPENDENCY_OF` relationship on it (SPDX 2.3). Confirmed via unit test — no `cargo` subprocess needed if the fix relies solely on manifest parsing OR gated behind `MIKEBOM_CARGO_RESOLVE_FEATURES=1` env var if the fix requires shell-out.

**Acceptance Scenarios**:

1. **Given** a Cargo workspace with `[dependencies] foo = { optional = true }` + `[features] default = ["foo"]`, **When** the operator runs `mikebom sbom scan --path <ws>`, **Then** the emitted `foo` component carries `scope: "runtime"` in CDX / no `OPTIONAL_DEPENDENCY_OF` relationship in SPDX 2.3 / `LifecycleScope::Runtime` internally.
2. **Given** the same workspace, **When** the operator emits SPDX 2.3, **Then** the annotation `mikebom:optional-derivation = "cargo-optional-true"` is ABSENT for `foo` (mikebom is no longer claiming the dep is optional-in-effect).
3. **Given** a workspace where member A declares `foo` optional but member B has a runtime chain reaching `foo`, **When** scanned, **Then** `foo` is `scope: "runtime"` (B's runtime need overrides A's declaration).

---

### User Story 2 - Truly-optional dep still classified Optional (Priority: P1)

An operator scans a Cargo workspace where a dep is declared `optional = true` AND is not activated by any default feature AND no cross-member runtime chain reaches it. The emitted SBOM correctly carries `scope: "excluded"` (CDX) / `OPTIONAL_DEPENDENCY_OF` (SPDX 2.3) — matching Cargo's actual behavior of not compiling this dep by default.

**Why this priority**: m179 shipped Optional classification for a legitimate reason — operators want to distinguish "this dep IS in the deployable artifact" from "this dep is only pulled in under an opt-in feature nobody's enabling by default." The fix must preserve that signal for the actually-optional case; it must not throw the baby out with the bathwater by reverting m179 entirely. P1 because losing m179's signal for truly-optional deps also causes downstream miscategorization (over-inclusion of unshipped components in vuln reports).

**Independent Test**: Construct a Cargo workspace with `[dependencies] bar = { optional = true }` and NO `[features]` reference (or a feature reference not in `default`). Scan. Assert `bar`'s emitted component has `scope: "excluded"` in CDX / `OPTIONAL_DEPENDENCY_OF` in SPDX 2.3 / `LifecycleScope::Optional` internally + `mikebom:optional-derivation = "cargo-optional-true"` annotation.

**Acceptance Scenarios**:

1. **Given** a workspace with `[dependencies] bar = { optional = true }` + no `[features]` activation, **When** scanned, **Then** `bar` is `scope: "excluded"` and carries the `mikebom:optional-derivation` annotation.
2. **Given** a workspace with an optional dep guarded behind a non-default feature (`[features] enable-bar = ["bar"]`), **When** scanned WITHOUT `--features enable-bar` context, **Then** `bar` is `scope: "excluded"` (the feature isn't on by default).

---

### User Story 3 - Non-Cargo scans are byte-identical (regression guard) (Priority: P1)

An operator scans any non-Cargo project. The emitted SBOM is byte-identical versus pre-m205 output.

**Why this priority**: The fix touches the cargo reader only. No other ecosystem reader should shift. Byte-identity guard prevents the fix from cascading unintended change elsewhere. P1 because it's a hard delivery-safety requirement.

**Independent Test**: Scan the existing non-Cargo public_corpus fixtures (go-cobra, npm-express, maven-guice, python-flask). Assert byte-identical output vs alpha.63 golden.

**Acceptance Scenarios**:

1. **Given** any non-Cargo fixture in `mikebom-cli/tests/fixtures/public_corpus/` (go-cobra, npm-express, maven-guice, python-flask), **When** scanned post-fix, **Then** the emitted CDX / SPDX 2.3 / SPDX 3 outputs are byte-identical to pre-m205 (or diff limited to `mikebom:generation-context` timestamp fields per determinism controls).

---

### Edge Cases

- **`[dependencies] foo = { optional = true, features = ["bar"] }`** — the `features = [...]` under an optional dep only activates when the dep itself is activated. Same rule as base case: check whether any enabled feature activates `dep:foo`.
- **Workspace inheritance (`optional.workspace = true`)** — resolve to the workspace's declaration first, then apply the same feature-activation check.
- **Implicit features (Cargo 1.60+): `[dependencies] foo = { optional = true }` implicitly creates feature `foo = ["dep:foo"]` in the manifest's `[features]` table unless the manifest explicitly opts out.** — mikebom's fix must account for this: if `default` includes `foo` (which activates the implicit feature `foo`, which activates `dep:foo`), the dep IS runtime-activated. Same principle applies to any recursive `[features]` chain that reaches `dep:foo`.
- **Cargo profile: `[profile.release] strip = "symbols"` etc. don't affect dep activation** — irrelevant to this fix.
- **`--no-default-features` at build time** — invisible to a lockfile-static scan; the fix assumes the default feature set is what the operator built with (industry-standard assumption; matches Cargo's own default behavior). Documented in Assumptions.
- **`cargo metadata` returns error / cargo not on PATH** — fall back to the pre-m205 name-only classification (with a WARN log) rather than aborting the scan; Constitution Principle III (fail closed with graceful degradation). Non-Cargo scans are unaffected regardless.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The cargo reader MUST resolve feature activation for each `optional = true` dep before marking it `LifecycleScope::Optional`. Deps activated by the enabled feature set MUST classify as `LifecycleScope::Runtime` (or the appropriate `Build`/`Development` scope per pre-m205 semantics).
- **FR-002**: The resolution MUST honor Cargo's canonical feature-resolution rules: (a) `default` feature list activation; (b) implicit features (Cargo 1.60+ auto-created `<name> = ["dep:<name>"]`); (c) explicit `[features]` chains referencing `dep:<name>`; (d) workspace-wide feature unification when a downstream member enables features on an upstream member.
- **FR-003**: When `optional = true` AND no enabled feature activates the dep, the classifier MUST still emit `LifecycleScope::Optional` + `mikebom:optional-derivation = "cargo-optional-true"` — preserving the m179 signal for truly-optional deps (US2).
- **FR-004**: When the cargo reader cannot resolve feature activation for a workspace (cargo binary absent, `cargo metadata` fails, workspace parse error), it MUST fall back to a safe default: treat the optional dep as `Runtime` (matching Cargo's actual worst-case build behavior where a feature user might enable it) and emit a WARN log naming the workspace + fallback reason. Constitution Principle III + IX: safe default is over-inclusion (dep visible to scanners), never silent under-inclusion.
- **FR-005**: Non-Cargo scans MUST NOT change output. Byte-identity guarantee per US3.
- **FR-006**: The fix MUST NOT depend on network access or external services. All feature resolution operates on local manifest + lockfile state (either via mikebom's parser OR a local `cargo` binary invocation).
- **FR-007**: Emission MUST be deterministic — given the same workspace state, two back-to-back scans MUST produce byte-identical output (feature-activation resolution is a pure function of manifest state, not scan-time state).
- **FR-008**: The fix MUST NOT introduce a new `mikebom:*` annotation. The existing `mikebom:optional-derivation` annotation continues to fire only when the dep is genuinely Optional (feature-inactive). No new emission surface.

### Key Entities *(include if feature involves data)*

- **Feature-resolved optional-dep classification**: internal in-process derived state per (workspace, package-name) tuple, replacing the current name-only `optional_names.contains(&pkg.name)` check with a feature-resolved `is_actually_optional_after_feature_resolution(workspace, pkg_name)` predicate. Not persisted; not surfaced to SBOM output.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The reporter's `test-vaultwarden` reproducer produces `scope: "runtime"` (not `scope: "excluded"`) on `reqsign-aws-v4@3.0.1` post-fix. Downstream vulns (RUSTSEC-2026-0194/0195 on `quick-xml@0.40.1`) reappear in scanner output.
- **SC-002**: A minimal synthetic fixture with `[dependencies] foo = { optional = true }` + `[features] default = ["foo"]` classifies `foo` as `LifecycleScope::Runtime`.
- **SC-003**: A minimal synthetic fixture with `[dependencies] bar = { optional = true }` + no default-feature activation classifies `bar` as `LifecycleScope::Optional` + `mikebom:optional-derivation = "cargo-optional-true"` annotation (m179 signal preserved for truly-optional).
- **SC-004**: `git diff --stat mikebom-cli/tests/fixtures/public_corpus/` post-fix shows ZERO drift for non-Cargo fixtures (go-cobra, npm-express, maven-guice, python-flask). Cargo fixture (rust-ripgrep) drift is limited to correcting any misclassified optional deps.
- **SC-005**: `./scripts/pre-pr.sh` wall-clock delta versus pre-m205 baseline is ≤ 10 seconds (allowing for the added feature-resolution step; if the fix shells out to cargo, it runs once per workspace scan).
- **SC-006**: PR description references `Closes #593`.

## Assumptions

- The scanner runs in an environment where `cargo` is (usually) available on PATH — mikebom's dev docs already assume this for the golden-regen harness. When absent, FR-004's graceful fallback applies.
- The operator's build uses the default feature set — mikebom scans the workspace at rest and cannot observe an operator's `--no-default-features` intent. This matches the industry-standard "SBOM reflects the default build" assumption (documented in Edge Cases). Operators building with non-default features can wait for a future `--features` flag if their use case demands it (out of scope for m205).
- Cross-workspace-member feature unification per Cargo 1.51+ resolver v2 semantics is the correct target. Resolver v1 is legacy (pre-2021 edition workspaces); mikebom optimizes for resolver v2 behavior. If a workspace opts into v1 (`resolver = "1"`), mikebom's classification may over-include (extra deps marked Runtime) — a bounded, safer failure mode per FR-004.
- No changes to the classifier plumbing across other cargo readers/emitters — only the classifier decision at `cargo.rs:1155` and its supporting data-collection helpers change.
- The `mikebom:optional-derivation` annotation's downstream consumer surface (CDX properties, SPDX 2.3 annotations, SPDX 3 annotations, parity catalog row C122) remains unchanged. The annotation's emission gate tightens (fires only for truly-Optional deps) but its shape is stable.
- Sibling classifiers for npm (m180), pip (m183), maven (m184) may share the same underlying assumption — investigating whether their optional-dep classification needs analogous feature-resolution work is deferred to follow-up milestones; m205 fixes cargo only.
