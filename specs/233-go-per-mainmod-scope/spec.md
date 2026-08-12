# Feature Specification: Go graph resolver — per-main-module `dependsOn` scoping

**Feature Branch**: `233-go-per-mainmod-scope`
**Created**: 2026-08-11
**Status**: Draft
**Input**: User description: Fix the Go graph resolver so each main-module's `dependsOn` edges reflect only what that module's own `go.mod` + `go.sum` declare — not an aggregate across every Go module found under the scan root. Verified upstream root cause of the leak that surfaced as "project-discovery leaks nested-module dependencies" in a reporter's ticket; project-discovery is a downstream victim.

## Background

Empirical repro (verified 2026-08-11 against `main` HEAD, waybill built post-m232): scan a directory tree with 4 Go modules — root at `.`, plus `hack/`, `tools/`, `deep/src/thing/` — each `require golang.org/x/text` at a distinct version (`v0.40.0` / `v0.37.0` / `v0.29.0` / `v0.25.0` respectively). The Go graph resolver emits, in `--project-discovery=all`:

```
root       dependsOn → x/text@v0.25.0 (should be v0.40.0)
                       plus phantom deps: deepthing, hack, tools
hack       dependsOn → x/text@v0.25.0 (should be v0.37.0)
tools      dependsOn → x/text@v0.25.0 (should be v0.29.0)
deepthing  dependsOn → x/text@v0.25.0 (correct)
```

Every main-module points at `x/text@v0.25.0` — the version declared only by the DEEPEST nested module (`deep/src/thing`). The reporter's observation ("last-writer-wins over filesystem walk order") is empirically accurate: in this fixture the deepest module is the last one processed, so its version wins for every module.

Additional bug: every main-module has phantom `dependsOn` edges to every OTHER main-module (root → deepthing, hack → tools, etc.). None of these `require` any other module in reality.

### Impact

Real-world reporter evidence from a Grafana scan (47 scan units, `--offline` mode as Kusari Inspector runs it):

- Root unit's SBOM contained `golang.org/x/text@v0.37.0` from `hack/` (k8s codegen tooling) and `klauspost/compress@v1.18.5` from `devenv/docker/blocks/prometheus_high_card/` — neither of which the root module builds. Each produced a false-positive vulnerability finding against the root component (`GO-2026-5970`, `GO-2026-5841`).
- Downstream consequence: inflated dependency counts, orphaned components (root's own declared version becomes an orphan while a nested module's version becomes the "attached" edge target), distorted graph-completeness heuristics.

### Not a project-discovery bug

The reporter's ticket title names `--project-discovery=root-only` / `strict` as the leak site. The `annotation_follow_up` pass under those modes is correct — the `waybill:workspace-member` annotation on the leaked `x/text@v0.25.0` correctly lists `["deep/src/thing"]`, and the annotation-check would decline to pull it in. It survives only because it's already reachable via the resolver's phantom `dependsOn` edge from the root. Fixing the resolver's per-module scoping closes the leak at its source; project-discovery needs no algorithm change.

## Clarifications

### Session 2026-08-11

- Q: When multiple main-modules declare different Go versions, does the emitted SBOM contain one `stdlib` per distinct version or one shared `stdlib`? → A: One `pkg:golang/stdlib@<version>` per distinct `go <version>` declaration found across the scan. Each main-module's `dependsOn` points at the stdlib component matching its own `go.mod`'s Go version. Same-version modules share; different-version modules get distinct stdlib components. Rationale: Go's actual toolchain builds each module against its declared version's stdlib; a single canonical stdlib would be a fresh mis-attribution of the same class this milestone closes.
- Q: When module A `replace`s a required dep with a sibling main-module's local path, what does A's `dependsOn` look like? → A: A `dependsOn` the sibling main-module's PURL directly (e.g., `pkg:golang/some.example.com/B@v0.0.0-unknown`). B's transitive graph flows via B's own `dependsOn` list, not A's. This matches Go's own `go mod graph` behavior and preserves the workspace-local semantic. Rejected alternatives: inlining B's transitive graph into A's edges (flattens the model, diverges from `go mod graph`); or ignoring the replace and pointing at the original required PURL (loses provenance visibility).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Multi-module Go tree emits truthful per-module edges (Priority: P1)

A CI operator scans a repository containing multiple Go modules (a common shape for platform repos where a `hack/`, `tools/`, `docs/`, or `devenv/` subtree carries its own `go.mod` for dev tooling). The emitted SBOM has one main-module component per `go.mod` found, and each main-module's `dependsOn` list reflects only the packages that module's OWN `go.mod` + `go.sum` declare — no bleed from sibling or nested modules.

**Why this priority**: This is the source of every downstream symptom the reporter identified (false-positive vulns, phantom `dependsOn` edges, orphaned true-version entries, project-discovery leak). Fixing it collapses the entire failure family into one closure.

**Independent Test**: Scan the reporter's minimal repro fixture (4 nested Go modules, each `require golang.org/x/text` at a distinct version). Assert every main-module's `dependsOn` for `x/text` names the version that module's own `go.mod` declares — no shared `v0.25.0` (or any other single version) across all four main-modules. Emit MUST be identical across `--project-discovery=all` / `root-only` / `strict` modulo the filtering project-discovery already does.

**Acceptance Scenarios**:

1. **Given** the reporter's minimal repro (4 modules, 4 distinct `x/text` versions), **When** the scan runs with `--project-discovery=all --offline`, **Then** the emitted SBOM's `dependencies[]` contains:
   - `root dependsOn: x/text@v0.40.0` (plus stdlib)
   - `hack dependsOn: x/text@v0.37.0` (plus stdlib)
   - `tools dependsOn: x/text@v0.29.0` (plus stdlib)
   - `deepthing dependsOn: x/text@v0.25.0` (plus stdlib)
   No main-module's `dependsOn` list contains any other main-module.
2. **Given** the same fixture, **When** scanned with `--project-discovery=root-only`, **Then** the emitted SBOM contains exactly the root main-module component plus the packages the root actually requires (`x/text@v0.40.0`, `stdlib`). Zero components from any nested module survive.
3. **Given** the same fixture, **When** scanned with `--project-discovery=strict`, **Then** the emitted SBOM matches the root-only case for this fixture (no `go.work` present).
4. **Given** the same fixture with an added `go.work` listing `use ( . ./tools )` (excluding `hack/` and `deep/src/thing/`), **When** scanned with `--project-discovery=root-only`, **Then** the emitted SBOM contains root's dependencies (`x/text@v0.40.0`) AND tools's dependencies (`x/text@v0.29.0`, because `tools` is a workspace member); zero versions from `hack/` or `deep/src/thing/` appear.
5. **Given** the reporter's Grafana-shape scan (47 scan units, `--offline`), **When** the root unit is scanned with `--project-discovery=root-only`, **Then** the emitted SBOM contains zero `x/text` entries at versions other than the one root's go.mod declares.

---

### User Story 2 — `waybill:workspace-member` annotation stays accurate (Priority: P2)

Every emitted Go component's `waybill:workspace-member` annotation names ONLY the workspace directories that actually contain the source file(s) where the component was discovered. A `x/text@v0.40.0` component discovered via the root's `go.sum` carries `waybill:workspace-member: ["."]`; one discovered via `deep/src/thing/go.sum` at a different version carries `["deep/src/thing"]`. When the same package + version appears in multiple modules' go.sums, the annotation names all contributing directories — no last-writer-wins collapse.

**Why this priority**: The workspace-member annotation is what m176 established as the "who owns this component" signal downstream consumers (including project-discovery) rely on. Without this fix, even a scan without the FR-001 leak could still mis-attribute components across the workspace-member axis.

**Independent Test**: Same fixture as US1 with an additional twist — arrange the fixture so `x/text@v0.29.0` is declared by BOTH `tools/` AND `hack/` (two modules with matching version). Assert the resulting `x/text@v0.29.0` component's `waybill:workspace-member` annotation is `["hack", "tools"]` (sorted union), not one arbitrary directory.

**Acceptance Scenarios**:

1. **Given** two nested modules requiring the same package + version, **When** the scan runs, **Then** the resulting component's `waybill:workspace-member` annotation is a sorted deduplicated union of the two directories.
2. **Given** the reporter's minimal repro, **When** the scan runs, **Then** every `x/text@<V>` component's `waybill:workspace-member` annotation names EXACTLY the module directory that declared that version (`["."]` for v0.40.0, `["hack"]` for v0.37.0, etc.).

---

### Edge Cases

- **`go.work` present but empty or malformed**: If the workspace declaration is unparseable, the resolver falls back to per-module scoping as if no `go.work` existed. Warn-and-continue; no error.
- **`go.mod` present but no `go.sum`**: A module that has never been resolved contributes zero `dependsOn` edges beyond what its own `go.mod` `require` lines name. No cross-module bleed.
- **`replace` directives**: When module A `replace`s a dep with a local path pointing at a sibling main-module, A's `dependsOn` list contains the sibling's PURL directly per Clarifications §2 / FR-002. The sibling's own transitive graph flows via its own `dependsOn` list — NOT inlined into A's edges. This preserves workspace-local semantics and matches `go mod graph` output.
- **Shared `go.sum` across a `go.work` workspace**: In Go workspace mode, `go.sum` entries are shared across `use`-listed modules. The resolver MUST NOT interpret this as "every workspace member depends on every entry"; each member's `dependsOn` still reflects only what its own `go.mod` transitively requires.
- **Circular `require` between modules in the same workspace**: Rare but legal via `replace` directives. Preserve the edges as declared; don't dedupe or drop.
- **`stdlib` component**: continues to appear in every main-module's `dependsOn` list. Per Clarifications §1, the emitted SBOM contains one `pkg:golang/stdlib@<version>` component per distinct `go <version>` declaration found across the scan. Same-version modules share a `stdlib` component; different-version modules `dependsOn` distinct stdlib components each matching their own `go.mod`. Never fabricate a "canonical" stdlib across mixed-version modules.
- **Same package at same version in multiple modules**: One component emitted, `waybill:workspace-member` names all contributing directories (US2 assertion).
- **`--offline` mode with empty module cache**: The reporter's original repro. Even with no cache to consult, per-module scoping MUST hold. The resolver can degrade to "unresolved" for transitive deps it can't reach, but MUST NOT synthesize wrong edges to fill the gap.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For each Go main-module component in the emitted SBOM, the `dependsOn` list MUST include only components whose PURL name is directly required (or transitively required) by that module's OWN `go.mod` + `go.sum`, at the version those files declare. No cross-module bleed.
- **FR-002**: For each Go main-module component, the `dependsOn` list MUST NOT include any OTHER Go main-module UNLESS the current module explicitly `require`s that other module. Per Clarifications §2, `replace` directives pointing at a sibling main-module's local path DO produce a `dependsOn` edge to that sibling's PURL (not to the original required PURL, and not to the sibling's inlined transitive graph) — this matches Go's own `go mod graph` behavior.
- **FR-003**: When two Go modules under the scan root require the same package at DIFFERENT versions, the emitted SBOM MUST contain one component per (package, version) tuple. Each main-module's `dependsOn` MUST reference the version its own manifests declare.
- **FR-004**: When two Go modules require the same package at the SAME version, the emitted SBOM MUST contain one deduplicated component. Its `waybill:workspace-member` annotation MUST be a sorted, deduplicated union of every contributing module's workspace directory.
- **FR-005**: The FR-001..FR-004 guarantees MUST hold identically across `--project-discovery=all` / `root-only` / `strict`. project-discovery may filter WHICH main-modules appear in the emitted SBOM, but MUST NOT change the edge shape of the ones that do survive.
- **FR-006**: The FR-001..FR-004 guarantees MUST hold identically in `--offline` mode. When the module cache is empty and the resolver can't reach a transitive dep, it MUST record that dep as unresolved rather than substituting a wrong version from a sibling module.
- **FR-007**: The `waybill:graph-completeness` document-scope annotation MUST accurately reflect the post-fix graph. Orphan counts that were artifacts of the mis-attribution bug (root's declared-version orphaned while sibling's version was attached) MUST disappear.
- **FR-008**: The emitted SBOM MUST contain one `pkg:golang/stdlib@<version>` component per distinct Go version declared across the scan's main-modules. Each main-module's `dependsOn` MUST reference the stdlib component matching the Go version its OWN `go.mod` declares. Same-version modules share; different-version modules get distinct stdlib components. Never emit a single canonical stdlib when the scan contains mixed Go versions.

### Key Entities

- **Go main-module component**: A component tagged `waybill:component-role: "main-module"` derived from a `go.mod` file. Its PURL takes the form `pkg:golang/<module-path>@<version>` or falls back to `v0.0.0-unknown` when `go describe` can't produce one (per milestone 053).
- **Per-module dependency graph**: The transitive closure of a specific main-module's `go.mod` + `go.sum` entries. Distinct from the SBOM-wide dependency graph.
- **Workspace-member annotation**: The `waybill:workspace-member` value on each component, populated per m176 from `evidence.source_file_paths`. Under FR-004, a shared component's annotation is a union across all contributing modules.
- **`go.work` workspace declaration**: A file at any ancestor of a Go module that declares which sibling modules the toolchain treats as workspace members. Affects Go's own resolution behavior; MUST be honored per m161 semantics.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reporter's minimal 4-module repro fixture, running `waybill sbom scan --offline --project-discovery=all` produces an emitted SBOM where each of the 4 main-modules' `dependsOn` for `x/text` names the version that module's own `go.mod` declares. Concretely: root → v0.40.0, hack → v0.37.0, tools → v0.29.0, deepthing → v0.25.0. Measured by jq inspection of the emitted `.dependencies[]`.
- **SC-002**: On the same fixture, `--project-discovery=root-only` and `--project-discovery=strict` each produce an emitted SBOM with exactly the root's declared package versions (no `x/text` versions other than `v0.40.0`). Pre-fix baseline (verified 2026-08-11): both modes emit `v0.25.0` AND `v0.40.0`. Post-fix target: `v0.40.0` only.
- **SC-003**: On the reporter's Grafana-shape scan (47 scan units, `--offline`), the root unit's emitted SBOM contains zero `x/text` entries at versions other than the version root's `go.mod` declares. Pre-fix baseline: at least `v0.37.0` from `hack/` bleeds in. Post-fix target: only the root's declared version. Measured by re-scan against a local Grafana clone with the milestone-233 binary.
- **SC-004**: On the same fixture, zero `klauspost/compress` component versions other than those the root's `go.mod` transitively declares appear in the root unit's SBOM. Pre-fix baseline: `v1.18.5` bleeds in from `devenv/docker/blocks/prometheus_high_card/`. Post-fix target: root-declared versions only.
- **SC-005**: On any fixture, no Go main-module component's `dependsOn` list contains any OTHER Go main-module PURL from the same scan UNLESS the current module explicitly `require`s that other module. Pre-fix baseline (from the 4-module repro): every main-module points at every other main-module. Post-fix target: main-module edges reflect only actual `require` declarations.
- **SC-006**: The reporter's Grafana root-unit SBOM's `waybill:graph-completeness` orphan-reason inventory drops the specific classes attributable to the leak. Post-fix, orphaned components caused by the reporter's bug MUST disappear; residual orphans from other classes are unchanged.

## Assumptions

- The Go graph resolver's fix is scoped to `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs` and its call sites in the golang reader. No changes to `project_discovery/filter.rs` are needed (verified 2026-08-11 by reading the annotation-check condition — it correctly declines to pull in a component whose `workspace-member` doesn't overlap the root's in-scope directories; the leak is purely upstream).
- The FR-004 `waybill:workspace-member` union semantic already ships in m176's `tag_components_with_workspace_member` at `scan_fs/mod.rs:1290`. This milestone verifies the tagging pass still produces union values after the resolver fix — no new tagging code needed.
- No new Cargo dependencies. The fix is a scoping change in the existing graph-resolver code path.
- The `--project-discovery` flag added in a prior milestone remains unchanged. Its algorithm is correct; only the resolver's inputs to it need fixing.
- Test fixtures use synthetic `example.com/mikebomfixture/*` module paths per memory `feedback_fixture_synthetic_package_names`. Real coordinates (like `golang.org/x/text` in the reporter's ticket) are used only for repro verification / documentation, not committed fixtures.
- Grafana verification (SC-003, SC-004) is a manual step performed once by the implementer; not automated into CI. The synthetic-fixture assertions (SC-001, SC-002, SC-005) are the CI regression signal.
- The `--offline` mode assumption for reproducibility: the resolver's per-module scoping behavior MUST NOT depend on whether the module cache is populated. FR-006 encodes this — the fix must not accidentally introduce a "correct only when online" regression.
- Existing golden fixtures under `waybill-cli/tests/fixtures/golden_inputs/golang/` may need updating to reflect the fixed per-module edges. Determined at implementation time.
