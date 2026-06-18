# Feature Specification: Smarter root component selection for polyglot + multi-module Go workspace scans

**Feature Branch**: `127-smarter-root-pick`
**Created**: 2026-06-17
**Status**: Draft
**Input**: User description: "Smarter BOM-subject root component selection for polyglot and multi-module Go workspace scans. Today the metadata.component / SPDX documentDescribes ladder falls through to the synthetic placeholder or Maven scan_target_coord when multiple main-module-tagged components exist. This produces wrong roots for two reproducible bug classes: polyglot repos like argo-workflows where Go, Java/Maven, and npm sub-projects coexist (Maven wins over Go main-module, issue 366); and multi-module Go workspaces like opentelemetry-collector where 50-plus nested go.mod files exist (alphabetic leaf wins over repo-root module, issue 367). Operators want the SBOM root to identify the project primary deliverable. Closes 366 and 367."

## Clarifications

### Session 2026-06-17

- Q: When the smarter heuristic falls through to a non-main-module subject (Maven `scan_target_coord` or `pkg:generic` placeholder) despite ≥1 main-module being detected, should the scan emit a warning? → A: Yes — warn on any fall-through past a detected main-module to a non-main-module subject (broader auditability than ecosystem-priority ties alone).
- Q: When `--bind-to-source` is in effect, does the new root-selection heuristic affect the binding subject? → A: Yes — binding subject moves with the new root. Pre-existing operator scripts that bound the wrong identity (Maven coord on argo-workflows, leaf submodule on otel-collector) will break with the fix; documented as a behavior change in the CHANGELOG. The binding feature's goal is "bind the subject"; the subject is now correct.
- Q: Should every root-selection heuristic carry a numeric confidence value (analogous to mikebom's existing CDX `evidence.identity.confidence` channel), so downstream consumers can decide programmatically whether to trust the auto-pick? → A: Yes. Each named heuristic gets a fixed confidence in `[0.0, 1.0]`: operator-override = 1.0; single-main-module = 1.0; repo-root-main-module = 0.95; longest-common-prefix = 0.80; ecosystem-priority = 0.70; maven-scan-target-coord = 0.60; synthetic-placeholder = 0.30. The annotation includes both the heuristic name AND the confidence.
- Q: When the Maven `pom.xml` reader (milestone 070) emits a main-module AND the JAR walker also emits a matching `scan_target_coord` for the same coord, how do we resolve the duplicate signal? → A: Dedupe at the source — suppress the `scan_target_coord` synthesis when a Maven reader main-module already covers the same coord. One signal per coord, no duplicate FR-007 case-(c) warning for pure-Java repos.
- Q: Does the new heuristic apply to image-tier scans (`mikebom sbom scan --image …`) as well as source-tier scans? → A: Source-tier only. Image-tier scans (the `scan_image` code path) use a different root-selection mechanism (the image reference itself); this feature explicitly does NOT touch image-tier behavior.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Multi-module Go workspace picks the repo-root module as SBOM subject (Priority: P1)

An operator scans a Go project that contains many nested `go.mod` files (a multi-module workspace pattern, like `go.opentelemetry.io/collector` with 55 internal modules). The scan produces an SBOM whose root component identifies the **top-level module** declared at the repo root — not an alphabetically-first leaf submodule.

**Why this priority**: This is the most common multi-module Go pattern in the ecosystem (every major project that publishes coordinated sub-libraries from one repo uses it: opentelemetry-collector, kubernetes/* sigs.k8s.io subprojects, hashicorp libraries, prometheus). Downstream consumers that key software identity on the SBOM root (vulnerability matching, license attribution, supply-chain tracking) silently misroute on every one of these scans today. Fixing this single behavior unblocks correct SBOMs for the largest cohort of affected projects.

**Independent Test**: On a fresh clone of `open-telemetry/opentelemetry-collector@v0.105.0`, running `mikebom sbom scan --path . --format spdx-2.3-json` produces a SBOM whose `documentDescribes` package has `name=go.opentelemetry.io/collector`, `versionInfo=v0.105.0`, and `purl=pkg:golang/go.opentelemetry.io/collector@v0.105.0`. The same correct root must appear in CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes`, and SPDX 3 `rootElement` for the same scan.

**Acceptance Scenarios**:

1. **Given** a Go repo with multiple `go.mod` files where exactly one sits at the repo root, **When** the operator scans without override flags, **Then** the SBOM root identifies the repo-root module.
2. **Given** a Go repo with multiple `go.mod` files and **none** at the repo root (all sub-packaged), **When** the operator scans without override flags, **Then** the SBOM root identifies the same module a deterministic tiebreaker selects (the module whose manifest-file path is the longest common path prefix of all detected main-module manifest-file paths); if no clear LCP winner exists, the existing synthetic placeholder fallback fires AND the scan emits a warning that operator override is recommended.
3. **Given** any of the above scenarios, **When** the operator passes `--root-name` and/or `--root-purl-type`, **Then** the operator override wins over the automatic selection (no behavior regression for the milestone-077 / `--root-purl-type` paths).
4. **Given** any of the above scenarios, **When** the same scan is emitted to all three formats in one invocation, **Then** the root component identity (name, version, PURL) is byte-identical across CDX, SPDX 2.3, and SPDX 3.

---

### User Story 2 - Polyglot repo prefers Go main-module over Maven/npm sub-projects (Priority: P1)

An operator scans a polyglot repo whose primary deliverable is a Go application but which also contains Java/Maven and/or npm sub-projects (test clients, example SDKs, frontends). The scan produces an SBOM whose root component identifies the **Go main-module** declared by the repo-root `go.mod` — not the Maven coord the JAR walker happened to surface or an npm sub-project main-module.

**Why this priority**: Same severity as US1 — the SBOM root is the consumer's identity key for vulnerability matching and license attribution. Picking the test-client Maven artifact or a npm UI sub-project as the SBOM subject silently routes vulnerability scanners and SCA tools at the wrong supply-chain identity. Two affected projects in the wild that have confirmed reproducible scans: argo-workflows (4 main-modules: 1 Go + 1 Maven + 2 npm; Maven wins today) and likely any K8s controller repo that ships a TypeScript dashboard alongside.

**Independent Test**: On a fresh clone of `argoproj/argo-workflows@v3.5.5`, running `mikebom sbom scan --path . --format spdx-2.3-json` produces a SBOM whose `documentDescribes` package has `name=github.com/argoproj/argo-workflows/v3`, `versionInfo=v3.5.5`, and `purl=pkg:golang/github.com/argoproj/argo-workflows/v3@v3.5.5`.

**Acceptance Scenarios**:

1. **Given** a repo with both a Go main-module rooted at the repo top AND a Maven `scan_target_coord` from the JAR walker, **When** the operator scans without override flags, **Then** the SBOM root identifies the Go main-module.
2. **Given** a repo with a Go main-module rooted at the repo top AND npm main-modules from sub-directories, **When** the operator scans without override flags, **Then** the SBOM root identifies the Go main-module.
3. **Given** a repo with multiple ecosystems' main-modules and NO Go main-module (e.g., a Python+Java polyglot project), **When** the operator scans without override flags, **Then** the SBOM root identifies the main-module whose source file sits at the repo root; if multiple ecosystems claim the repo root, the deterministic priority order (the same for every operator on every machine) selects one of them and emits a warning recommending `--root-name`/`--root-purl-type` override.
4. **Given** any of the above scenarios, **When** the operator passes a `--root-name` override, **Then** the operator wins.

---

### User Story 3 - Transparency annotation surfaces which heuristic selected the root (Priority: P2)

An operator inspecting an SBOM emitted by the smarter selection wants to see which heuristic picked the root component, so they can decide whether to override or trust the auto-selection.

**Why this priority**: The new selection logic introduces multiple tiebreakers (repo-root presence, longest-common-prefix, ecosystem priority). Surfacing the chosen heuristic via the existing `mikebom:*` annotation channel makes the decision auditable and helps operators understand when the auto-pick is reliable vs. when they should override. Lower priority than US1 + US2 because the bugs are fixable without this, but the transparency is required by Constitution Principle X for behavioral changes that affect identity.

**Independent Test**: Every SBOM whose root was chosen by the new heuristic (i.e., NOT by the existing count==1 fast path, NOT by operator override) carries a document-scope annotation that names the heuristic used.

**Acceptance Scenarios**:

1. **Given** US1's repo-root tiebreaker fires, **When** the SBOM is emitted, **Then** a document-scope annotation records that the root was selected via "repo-root main-module" heuristic.
2. **Given** US2's ecosystem-priority tiebreaker fires, **When** the SBOM is emitted, **Then** a document-scope annotation records that the root was selected via "ecosystem priority (Go preferred)" heuristic.
3. **Given** the existing milestone-053 single main-module fast-path fires (no tiebreaker needed), **When** the SBOM is emitted, **Then** no new annotation is added (preserves byte-identity for single-module Go scans).
4. **Given** the operator passes `--root-name`, **When** the SBOM is emitted, **Then** no new heuristic annotation is added (the existing milestone-077 override audit is the right channel).

---

### Edge Cases

- **No main-modules detected at all** (no recognized ecosystem manifests): preserve the existing synthetic `pkg:generic/<target>@0.0.0` fallback exactly as today — this feature only changes behavior when count > 1.
- **Empty Go workspace** (a `go.work` file with no included modules): treat as no main-modules detected; fall back to existing behavior.
- **Cargo workspace with multiple `[workspace.members]` entries**: this is already the count > 1 case for cargo (per milestone 064); the new repo-root tiebreaker should apply uniformly across Go AND cargo workspaces, picking the workspace root (the `Cargo.toml` carrying `[workspace]` itself) as the SBOM root.
- **Symlinked submodules pointing at the same `go.mod`**: deduplicate by canonical path before counting; one effective main-module is the count==1 path.
- **Scanning a sub-directory of a repo (not the repo root)**: the "repo root" for tiebreaker purposes is the `--path` argument, not git's `.git`-discovered repo root. An operator who scans `path/to/subproject/` and there's a `go.mod` at that path gets that module as root.
- **`scan_target_coord` from JAR walker fires AND main-modules exist**: per US2, main-modules win. The `scan_target_coord` path remains the fallback when no main-modules exist (preserves existing behavior for pure-Java repos).
- **All ecosystems claim the repo root** (e.g., both `go.mod` AND `Cargo.toml` AND `pom.xml` sit at the repo root — a tooling-experiment polyglot): use the deterministic ecosystem-priority order and emit a warning.
- **Image-tier scans** (`mikebom sbom scan --image …`): out of scope. Image-tier scans use the `scan_image` code path and select the root from the image reference, not from main-module annotations. This feature MUST NOT change image-tier root selection — even when an image's filesystem contains a `go.mod` or buildinfo-extracted main-module, the image-tier root remains the image reference per existing behavior.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST add a `is_workspace_root: bool` signal to every component carrying `mikebom:component-role: "main-module"`, set to `true` when the component's defining manifest file (`go.mod`, `Cargo.toml`, `pom.xml`, `package.json`, `Gemfile`, `setup.cfg`/`pyproject.toml`) sits at the scan's `--path` root, `false` otherwise.

- **FR-002**: When the existing main-module-count tiebreaker at the CDX `metadata.component` builder evaluates `count > 1`, System MUST select the unique main-module whose `is_workspace_root == true` before falling through to the existing Maven `scan_target_coord` or synthetic placeholder branches.

- **FR-003**: When `count > 1` AND multiple main-modules have `is_workspace_root == true` (polyglot at the root), System MUST apply a deterministic ecosystem-priority order: Go > Cargo > Maven > npm > Pip > Gem > Generic. The first ecosystem in this order that has a `is_workspace_root == true` main-module wins.

- **FR-004**: When `count > 1` AND **no** main-module has `is_workspace_root == true` (every main-module is in a subdirectory), System MUST compute the longest common path prefix of all main-module-defining manifest file paths; if exactly one main-module's manifest path equals that prefix, select it. Otherwise fall through to the existing fallback ladder.

- **FR-005**: The SPDX 2.3 `documentDescribes` selection and the SPDX 3 `rootElement` selection MUST apply the same selection algorithm as CDX `metadata.component`, so that all three formats agree on the root identity for any single scan invocation (single-pass guarantee from milestone 053).

- **FR-006**: When the smarter selection picks a root via the FR-002 (repo-root), FR-003 (ecosystem-priority), or FR-004 (longest-common-prefix) tiebreaker, the emitted SBOM MUST carry a document-scope `mikebom:root-selection-heuristic` annotation whose value is a JSON object `{"heuristic": <name>, "confidence": <float>}` with the heuristic-name one of: `"repo-root-main-module"` (confidence 0.95), `"ecosystem-priority"` (confidence 0.70), `"longest-common-prefix"` (confidence 0.80), `"maven-scan-target-coord"` (confidence 0.60), or `"synthetic-placeholder"` (confidence 0.30). When the pre-existing single-main-module fast path fires, no annotation is emitted (preserves byte-identity for `"single-main-module"`, conceptual confidence 1.0). When the operator override (`--root-name`) fires, no annotation is emitted (the override audit channel is the right place per FR-008, conceptual confidence 1.0).

- **FR-007**: System MUST emit a `tracing::warn!` log at scan-end whenever ≥1 main-module was detected but the auto-pick fell through to a non-main-module subject — that is, in ANY of these cases: (a) FR-003 fires with multiple competing ecosystems at the repo root (selected vs. competing ecosystems both named), (b) FR-004's longest-common-prefix tiebreaker has no clear winner and the ladder falls through to Maven `scan_target_coord` or `pkg:generic` placeholder, or (c) the existing Maven `scan_target_coord` branch is selected with ≥1 main-module also present. In every case the warning MUST name the picked subject AND list the loser main-modules' PURLs, recommending the operator pass `--root-name`/`--root-purl-type` for deterministic control.

- **FR-008**: The existing milestone-077 `--root-name` / `--root-version` / `--root-purl-type` / `--no-root-purl` override flags MUST continue to win over any new heuristic — operator override is the topmost branch of the priority ladder.

- **FR-009**: The existing single-main-module fast path (count == 1) MUST be preserved exactly. Scans of single-module Go projects, single-crate Cargo projects, and other single-main-module ecosystems MUST produce byte-identical SBOMs to today's output.

- **FR-010**: For symlinked submodules pointing at the same canonical `go.mod` / `Cargo.toml` / etc., System MUST canonicalize paths before deduplication, so symlink trickery doesn't inflate the main-module count past 1.

- **FR-011**: When `--bind-to-source` is in effect, the `SourceDocumentBinding` envelope's subject MUST be the root component as selected by the heuristic ladder above (operator override > single-main-module fast path > repo-root tiebreaker > ecosystem-priority > longest-common-prefix > Maven `scan_target_coord` > synthetic placeholder). The binding subject does NOT receive special freeze-old-behavior treatment — when the new heuristic picks a different root than today's behavior, the binding follows the new root. Affected scans are exactly the two reproducible bug classes (#366 polyglot, #367 multi-module workspace); operator binding scripts targeting the old (wrong) subject on those projects MUST be updated and the behavior change MUST be flagged in the CHANGELOG.

- **FR-012**: When the Maven `pom.xml` reader (milestone 070) emits a main-module-tagged component whose PURL matches the JAR walker's `scan_target_coord`, System MUST suppress the JAR-walker `scan_target_coord` synthesis for that scan. The Maven reader's main-module is the canonical signal for any coord it claims. This dedup happens at signal generation, before the metadata.component / documentDescribes / rootElement ladder runs, so FR-007 case-(c) only fires when `scan_target_coord` is the WINNER AND describes a coord NOT covered by any Maven main-module — i.e., the genuinely-ambiguous case worth warning about.

### Key Entities

- **Main-module component**: A `ResolvedComponent` whose `extra_annotations` carry `mikebom:component-role: "main-module"`. Already exists today. This feature adds the `is_workspace_root` boolean and the selection-time tiebreakers.

- **Root-selection heuristic**: A `(name, confidence)` pair where the name is one of the values defined by FR-006 and the confidence is a fixed float in `[0.0, 1.0]` indicating how trustworthy the selection is. Drives the document-scope `mikebom:root-selection-heuristic` annotation. Modeled after mikebom's existing CDX `evidence.identity.confidence` channel (e.g., the `confidence: 0.85, technique: "package-database"` shape on the kubelb scan).

- **Ecosystem priority order**: An ordered list `[golang, cargo, maven, npm, pip, gem, generic]` used by FR-003 when multiple ecosystems claim the repo root.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A fresh scan of `open-telemetry/opentelemetry-collector@v0.105.0` produces an SBOM whose root component PURL is `pkg:golang/go.opentelemetry.io/collector@v0.105.0` (today: a deep-leaf sub-module).

- **SC-002**: A fresh scan of `argoproj/argo-workflows@v3.5.5` produces an SBOM whose root component PURL is `pkg:golang/github.com/argoproj/argo-workflows/v3@v3.5.5` (today: a Maven `argo-client-java-tests` coord).

- **SC-003**: A fresh scan of every project in the existing milestone-090 fixture set produces a root component PURL byte-identical to its current alpha.48 golden (zero regression on the single-main-module fast path). Verified by re-running the `cdx_regression`, `spdx_regression`, and `spdx3_regression` golden suites with NO `MIKEBOM_UPDATE_*` env vars set.

- **SC-004**: For every scan whose root selection used a new tiebreaker, the emitted SBOM carries a document-scope annotation containing BOTH the heuristic name AND a confidence value in `[0.0, 1.0]` per FR-006; for every scan whose root was selected by the pre-existing count==1 fast path OR by operator override, no such annotation is added (byte-identity preserved on the fast path; override audit channel handles override case).

- **SC-005**: Across CDX, SPDX 2.3, and SPDX 3 outputs from a single scan invocation, the root component's `name`, `version`, and `purl` are byte-identical (cross-format consistency).

- **SC-006**: Operator override (`--root-name`, `--root-purl-type`, `--no-root-purl`) wins over every new heuristic, verified by integration test on a multi-main-module repo with an override applied.

- **SC-007**: Any scan where ≥1 main-module was detected but the auto-pick falls through to a non-main-module subject (ecosystem-priority tie, no longest-common-prefix winner, or Maven `scan_target_coord` selection with main-modules present) produces a `tracing::warn!` log at scan-end naming the picked subject AND listing the loser main-modules' PURLs, recommending operator override.

## Assumptions

- The `--path` argument identifies the operator's intended "repo root" for tiebreaker purposes. We do NOT use `git`-discovered repo roots (`git rev-parse --show-toplevel`) because operators frequently scan sub-trees of larger monorepos, and the `git` root would be wrong for those cases. This matches the milestone-053 convention.

- Ecosystem priority order `[golang, cargo, maven, npm, pip, gem, generic]` reflects mikebom's source-tier readers' relative maturity AND the empirical observation that operators reporting wrong-root bugs are overwhelmingly working with Go primary projects whose secondary ecosystems (Java test clients, npm UIs) are project-internal rather than the deliverable. The order is documented in the spec and changeable by future spec revision, not by config knob.

- Existing `mikebom:*` annotation channel is the right place to surface the heuristic-used signal (Constitution Principle V: standards-native first, `mikebom:*` for transparency-only signals). No CDX or SPDX native field carries this notion.

- The existing milestone-053 `build_main_module_entry` correctly identifies main-modules across Go, Cargo (milestone 064), npm (milestone 066), pip (milestone 068), gem (milestone 069), and maven (milestone 070). This feature plugs into the existing main-module annotation; it does not invent a new detection mechanism.

- The fix lands in user-space Rust only; no eBPF or kernel-side change.

- No new Cargo dependencies. The path-comparison and canonicalization work uses `std::path::Path` + `std::fs::canonicalize` already pervasive in `scan_fs/`.

- 33 byte-identity goldens in `mikebom-cli/tests/fixtures/golden/` are EXPECTED to stay byte-identical (every fixture is a single-main-module project). Two NEW goldens will land for the two failing scenarios as integration-test fixtures.

- The `mikebom:root-selection-heuristic` annotation is a new catalog-row (C-row sequence TBD by the catalog at spec-time). Its addition is additive to the milestone-005 catalog and requires the matching Principle V audit narrative.

- This feature is **source-tier scope only**. Image-tier scans (`--image`), the binary-tier ELF/Mach-O/PE walker, and the eBPF build-trace pipeline all continue to use their existing root-selection mechanisms unchanged. The heuristic ladder defined here applies exclusively to the `scan_path` code path.
