# Feature Specification: Split-mode grouping strategies

**Feature Branch**: `219-split-modes`
**Created**: 2026-07-23
**Status**: Draft
**Input**: User description: "m219 — add --split=<mode> extensibility for grouping strategies (workspace default, directory new, ecosystem future)"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Directory-grouped split for polyglot repos (Priority: P1)

An operator scans a monorepo where multiple ecosystems coexist in the same directory (a common shape: `package.json` + `go.sum` in a scripts dir; `Cargo.toml` + `build.gradle` in an FFI shim dir; `pyproject.toml` + `package.json` in an ML-service dir). With the current milestone-215 `--split` behavior, they get one sub-SBOM per main-module — so the scripts dir produces TWO SBOMs (one npm, one go) even though a downstream consumer thinks of them as "the scripts dir" as a unit. The operator wants a `--split=directory` mode that groups all main-modules whose source directories are identical into ONE sub-SBOM per directory, regardless of ecosystem.

**Why this priority**: Real-world polyglot repos (Kubernetes controllers with a Go core + a package.json for build tooling; Rails apps with a Gemfile at root + package.json for asset pipeline; ML-serving apps with pyproject + package.json) generate a fan-out of tiny same-directory SBOMs today. Consumers organizing SBOMs by directory (Backstage, IDE plugins, source-repo tag-and-release automation) want one artifact per dir. This is the primary use case driving the m219 milestone.

**Independent Test**: Author a fixture with `<root>/services/api/{Cargo.toml, package.json}` — two main-modules in the same dir. Run `waybill sbom scan --path <root> --split=directory --output-dir out/`. Assert exactly ONE sub-SBOM file emitted for that dir (containing components from both Cargo AND package.json origins), plus the split-manifest.json listing the two contributing main-module PURLs under a single entry. Compare against `--split=workspace` (or bare `--split`) which produces TWO sub-SBOMs for the same fixture.

**Acceptance Scenarios**:

1. **Given** a polyglot fixture with `<root>/services/api/{Cargo.toml, package.json}` and `<root>/services/worker/{go.mod}`, **When** the operator runs `waybill sbom scan --path <root> --split=directory --output-dir out/`, **Then** exactly TWO sub-SBOMs are emitted: one for `services/api/` (containing both `pkg:cargo/api` and `pkg:npm/api` main-modules plus their transitive deps) and one for `services/worker/` (containing `pkg:golang/worker`).
2. **Given** the same fixture, **When** the operator runs `waybill sbom scan --path <root> --split --output-dir out/` (bare `--split`, no mode value), **Then** the output is byte-identical to what m215 alpha.67 produces today (THREE sub-SBOMs — one per main-module — proving default-behavior backward compatibility).
3. **Given** the polyglot fixture with `--split=directory`, **When** the resulting `split-manifest.json` is inspected, **Then** the entry for `services/api/` carries a `members: [{purl, source_dir}]` list of length 2 (one per contributing main-module), and the entry for `services/worker/` carries `members` of length 1 OR omits the field entirely (v1 schema behavior determined at plan phase — see Assumptions).

---

### User Story 2 - Extensibility: `--split=<mode>` accepts future grouping strategies without CLI-flag pollution (Priority: P2)

Waybill's operator base has diverse SBOM-consumer downstreams. Beyond "workspace" (per-main-module) and "directory" (per-source-directory), plausible future grouping strategies include: `--split=ecosystem` (one sub-SBOM per `pkg:<type>/*`, useful for security-team reviews organized by tech stack), `--split=owner` (one sub-SBOM per CODEOWNERS entry, useful for per-team compliance dashboards), `--split=custom=<path-to-config>` (operator-supplied grouping rules). The milestone-219 CLI-flag shape and internal grouping-strategy abstraction MUST accommodate future strategies without breaking the `--split=workspace` / `--split=directory` contracts landed in this milestone.

**Why this priority**: Anti-pollution + forward-compat. If we ship `--split=directory` as a discrete `--split-by-directory` boolean flag, adding `--split-by-ecosystem` and `--split-by-owner` later means three orthogonal-but-mutually-exclusive boolean flags on `waybill sbom scan --help` — degrades discoverability + adds validation complexity. A single `--split=<mode>` enum-style flag preserves a clean surface. Not P1 because the immediate user need is US1's directory mode; extensibility is what makes THIS milestone the last one that touches the CLI surface for this feature family (until a genuinely-new user need surfaces).

**Independent Test**: `waybill sbom scan --split=<invalid-mode>` emits a stderr error naming every accepted mode (`workspace`, `directory`). Adding a new hypothetical variant in a follow-up milestone requires editing only the enum variant + adding a group-key implementation branch; no changes to CLI parsing, help output structure, or documentation shape.

**Acceptance Scenarios**:

1. **Given** a scan invocation with `--split=nonexistent-mode`, **When** the CLI parses arguments, **Then** it exits with a non-zero status and a stderr message listing the two accepted values (`workspace`, `directory`).
2. **Given** the internal grouping abstraction is implemented as a single enum with a `group_key(&SubprojectRoot) -> String` method, **When** a hypothetical follow-up milestone adds an `Ecosystem` variant, **Then** the diff touches only: (a) the enum's variant list; (b) the `group_key` match arm; (c) the docs page's mode table; (d) a new test case. Zero changes required to the CLI-flag definition, split-manifest schema, or filename slug computation.

---

### Edge Cases

- **A directory contains only ONE main-module**: `--split=directory` and `--split=workspace` produce byte-identical output for that group (no `members[]` fanout).
- **Two main-modules in nested directories** (e.g., `services/api/` and `services/api/tests/`): they are DIFFERENT directories → different groups, one SBOM each. `--split=directory` does NOT collapse ancestor→descendant relationships.
- **Symlinked source dirs**: canonicalize both source paths before grouping. Two main-modules whose canonicalized paths differ produce two groups even if one is a symlink to the other's parent.
- **Empty source_dir** (subproject IS the scan root): all root-level main-modules with `source_dir == ""` collapse into ONE group under `--split=directory`. This is the desired behavior — a multi-ecosystem project at the scan root is ONE SBOM under directory mode.
- **Fallback: zero boundaries detected**: same behavior as m215 — WARN and fall through to single-SBOM emission. `--split=<mode>` has no bearing.
- **Merged group's ecosystems disagree on the "main" ecosystem**: filename slug for a group of 2+ ecosystems can't use m215's `<slug>.<ecosystem>` convention. The v1 filename shape is a plan-phase decision; the spec locks the semantic that the filename MUST be deterministic + must not clash with same-directory neighbor groups.
- **Multi-version main-modules in the same directory** (edge of an edge: e.g., `Cargo.toml` declares two workspace members that both point to files in the same nested dir): both members' contributions merge into the group. Split-manifest lists each member in `members[]`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CLI flag `--split` MUST accept an optional value argument — either `workspace` (default when the flag is passed bare) or `directory`. Bare `--split` (no value) MUST behave byte-identically to `--split=workspace` and to the alpha.67 `--split` implementation (backward compatibility contract).
- **FR-002**: `--split=workspace` MUST produce output byte-identical to alpha.67 `--split` on every existing m215 test fixture (SC-005 byte-identity guard). No golden regeneration required for this mode.
- **FR-003**: `--split=directory` MUST group main-modules by their canonicalized source directory. Grouping keys derived from `SubprojectRoot.source_dir` post-canonicalization (symlink resolution + relative-path normalization against the scan root).
- **FR-004**: For each group of N ≥ 1 main-modules under `--split=directory`, waybill MUST emit ONE sub-SBOM whose components + relationships are the union of the BFS-projections of every contributing main-module. Deduplication follows the same rules as m215's within-a-single-projection dedup (component-PURL uniqueness; relationship-tuple uniqueness).
- **FR-005**: The `split-manifest.json` schema MUST be extended (v1 → v2, OR v1 additive) with a `members` field on each entry listing every contributing main-module's `{purl, source_dir}`. For entries whose group covers exactly ONE main-module, `members` MAY be omitted (v1 shape preservation) OR included as a 1-element list (v2 shape) — the exact behavior is a plan-phase decision documented in the schema evolution note.
- **FR-006**: Filename convention for grouped sub-SBOMs MUST be deterministic and MUST NOT collide between neighbor groups in the same output dir. When a group covers exactly one main-module, the filename MUST match m215's `<slug>.<ecosystem>` convention (SC-005 byte-identity contract). When a group covers ≥2 main-modules, the filename MUST use a shape that doesn't reference any single ecosystem (candidates: `<dir-slug>.multi`, `<dir-slug>.<hash>`, `<combined-hash>`; final choice at plan phase).
- **FR-007**: The internal grouping abstraction MUST be an enum whose variants each provide a `group_key: fn(&SubprojectRoot) -> String` function. Adding a new grouping strategy in a future milestone MUST require touching only: (a) the enum's variant list; (b) the `group_key` implementation; (c) the docs page's mode-list table; (d) a new test scenario. Zero required changes to the CLI-flag parsing, the split-manifest schema (unless the new mode introduces new metadata), or the split-driver code.
- **FR-008**: An invalid `--split=<mode>` value MUST cause CLI parse to fail with a non-zero exit and a stderr error naming the accepted values (`workspace`, `directory`). The error message MUST NOT crash on malformed input (empty string, whitespace-only, uppercase mode names — treat as invalid).
- **FR-009**: `--split=directory` with a scan that has zero detected main-modules MUST fall back to the same single-SBOM emission path m215 uses today (WARN log, one SBOM in the output dir). Byte-identity contract with m215's fallback preserved.
- **FR-010**: The FR-013-style INFO log emitted at split-driver exit MUST include the mode string (`workspace` or `directory`) and the group count per mode. Format: `INFO split emission complete mode=<mode> groups=<N> total_main_modules=<M>` — consumers running `waybill` under `RUST_LOG=info` see the mode-decision + group-cardinality.
- **FR-011**: Documentation at `docs/user-guide/cli-reference.md#split` (or an equivalent reference doc) MUST be extended with: (a) the two accepted mode values; (b) a decision table describing when to choose each; (c) at least one worked example per mode showing the resulting `split-manifest.json` shape; (d) the extensibility contract for future modes so contributors adding a new variant know what surfaces to touch.

### Key Entities *(include if feature involves data)*

- **SplitMode**: enum with variants `Workspace` (default) and `Directory` (new). Each variant provides a `group_key(&SubprojectRoot) -> String` function that maps a main-module root to its grouping key. `Workspace::group_key` returns `SubprojectRoot::subproject_id()` (current m215 behavior — one group per root). `Directory::group_key` returns the canonicalized source-dir path string. Future variants (`Ecosystem`, `Owner`, `Custom`) plug into the same interface.
- **GroupedProjection**: a post-grouping structure holding one group's `key: String`, its `members: Vec<SubprojectRoot>` (length ≥ 1), and the union of the members' BFS-projected components + relationships. Feeds directly into the existing sub-SBOM emitter.
- **SplitManifest v2** (or v1-additive): extends the m215 `SplitEntry` shape with an optional `members: [{purl, source_dir}]` field. Present when a group covers ≥2 main-modules; may be omitted (m215-compat) or always-present (v2) — final choice at plan phase.
- **Filename slug for grouped sub-SBOMs**: deterministic function of the group's `(key, members)`. Single-member groups reuse m215's `<slug>.<ecosystem>` shape verbatim. Multi-member groups use a new shape (candidates in FR-006; final at plan phase).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a polyglot fixture with `<root>/services/api/{Cargo.toml, package.json}` + `<root>/services/worker/{go.mod}`, `waybill sbom scan --split=directory --output-dir out/` emits exactly 2 sub-SBOM files (one per directory) — verified via `ls out/*.cdx.json | wc -l == 2`.
- **SC-002**: On the same fixture, `waybill sbom scan --split=workspace --output-dir out/` emits exactly 3 sub-SBOM files (one per main-module) — verified via `ls out/*.cdx.json | wc -l == 3`.
- **SC-003**: On the same fixture with `--split=directory`, the emitted `split-manifest.json` for the `services/api/` group contains exactly 2 entries in its `members[]` array, and the sub-SBOM's `components[]` contains BOTH `pkg:cargo/api` AND `pkg:npm/api` main-modules — verified via `jq '.entries[] | select(.source_dir | endswith("services/api")) | .members | length' == 2` on the manifest.
- **SC-004**: The two sub-SBOMs from SC-001 have NO overlapping components — proves the directory-grouping BFS-projection preserves the m215 shared-deps semantics per-group.
- **SC-005**: Byte-identity: `waybill sbom scan --split` (bare) and `waybill sbom scan --split=workspace` produce output byte-identical to alpha.67 `waybill sbom scan --split` on every existing m215 test fixture. Zero goldens regenerate for these paths.
- **SC-006**: Invalid mode value: `waybill sbom scan --split=nonexistent-mode --output-dir out/` exits non-zero, prints a stderr error naming the two accepted values, and emits ZERO files to `out/`.
- **SC-007**: An INFO-level log emitted at split-driver exit contains the substring `mode=directory` when the operator passed `--split=directory` — verified via `RUST_LOG=info` capture in an integration test.
- **SC-008**: The `docs/user-guide/cli-reference.md#split` section documents both mode values, includes at least one worked example per mode, and lists the four surfaces a future-mode contributor must touch (per FR-007). Verified via a lint step that greps for the required section headings.
- **SC-009**: A synthetic extensibility test — hand-add a `#[cfg(test)] pub enum SplitMode { Workspace, Directory, TestOnlyEcosystem }` variant to the test module + the corresponding `group_key` match arm + a test scenario. The build succeeds and the scenario passes. Proves the enum-extension contract holds mechanically.

## Assumptions

- The primary use case driving m219 is polyglot monorepos where multiple ecosystems coexist in the same directory (Rails+webpack, K8s controllers with Go+Node build tooling, ML-service repos with Python+Node). These shapes are common at scale in enterprise monorepos where m215 already ships value.
- Backward-compat contract (SC-005): bare `--split` and `--split=workspace` MUST be byte-identical to alpha.67 `--split`. No golden regen for these paths. Any change requires an explicit spec update.
- The CLI-flag shape is `--split[=<mode>]` (optional value; default `workspace` when omitted). Rejected alternative: a separate `--split-by=<mode>` flag — pollutes the flag surface, requires validation coupling with `--split`.
- The internal grouping abstraction is an enum with a method (not a trait object), so no dynamic dispatch, no `Box<dyn>`, no lifetime gymnastics. Adding a variant is compile-time cheap.
- Filename convention when a group covers ≥2 main-modules is a plan-phase decision; three candidates surfaced in FR-006 (`<dir-slug>.multi`, `<dir-slug>.<hash>`, `<combined-hash>`). Locking one at plan phase should consider: filesystem-safe char set, readability for humans, clash-safety across neighbor groups in the same output dir, and manifest-side stability across scan runs.
- Split-manifest schema evolution is also plan-phase: options are (v1 additive: `members` optional, single-member groups omit it) or (v2 bump: `members` always present, schema URL changes). Consumer-impact tradeoff evaluated at plan phase.
- Docs surface: `docs/user-guide/cli-reference.md#split` — if that file doesn't exist yet (m215 may have documented `--split` elsewhere), the plan phase identifies the correct location and either extends it or adds a new page linked from README's SBOM interpretation section.
- Non-goals: (1) inventing new grouping strategies beyond `workspace` + `directory` in this milestone — extensibility is proven via SC-009 but v1 ships only 2 modes; (2) redesigning m215's BFS-projection algorithm — this milestone reuses it as a black-box per-member and unions the results; (3) supporting per-format grouping modes (e.g., "workspace for CDX + directory for SPDX") — future milestone if a user need surfaces; (4) split-manifest schema URL bump (v1 → v2) unless plan-phase concludes it's necessary — additive extension preferred to preserve consumer parsers.
- Not blocking m215/m216/m217/m218: this milestone is an ADDITIVE extension. Existing consumers see no change unless they explicitly pass a mode value.
