# Feature Specification: `--project-discovery=<mode>` — cap main-module discovery scope

**Feature Branch**: `220-project-discovery-scope`
**Created**: 2026-07-24
**Status**: Draft
**Input**: User description: "m220 — add --project-discovery=<mode> flag (all/root-only/strict). Cap main-module DISCOVERY scope while honoring ecosystem-native workspace-member declarations (Cargo workspaces, npm workspaces, go.work, Maven multi-module) at the root level."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Shallow scan of a polyglot repo root, ignoring nested independent projects (Priority: P1)

An operator scans a polyglot monorepo where the scan root contains a top-level project (e.g., a `Cargo.toml` at root) BUT also has nested independent projects buried deep in `services/api/{package.json}`, `tools/scripts/{Gemfile}`, `bench/{go.mod}`. Today, every discovered manifest becomes its own main-module → the emitted SBOM has 4 main-modules from the operator's perspective, but they only care about the root project. Under `--split=directory`, they get 4 sub-SBOMs; without `--split` they get a synthetic-`pkg:generic/<repo>` root and 4 main-module-tagged components. The operator wants a shallow mode where nested independent projects are IGNORED at discovery time — no components from them, no deps from them, just the root project.

**Why this priority**: This is the primary use case driving m220 — operators scanning a specific project inside a larger monorepo don't want the SBOM polluted with unrelated nested projects. Real-world shape: dev containers, mono repos with per-team subdirs, CI matrix scans where each job scopes to one project.

**Independent Test**: Author a fixture with `Cargo.toml` at root + nested `services/api/{package.json, package-lock.json}` + `services/worker/{go.mod, go.sum}`. Run `waybill sbom scan --path <root> --project-discovery=root-only`. Assert the emitted SBOM contains ONE main-module (the cargo one at root) and its transitive-dep set — but ZERO components from the npm or go projects (proves they're truly ignored, not just untagged). Compare against `--project-discovery=all` (default) which yields 3 main-modules + all their transitive deps.

**Acceptance Scenarios**:

1. **Given** a fixture with a root `Cargo.toml` + nested `services/api/package.json` + `services/worker/go.mod`, **When** the operator runs `waybill sbom scan --path <root> --project-discovery=root-only`, **Then** the emitted SBOM's `components[]` contains the root cargo main-module + its cargo transitive deps ONLY. No `pkg:npm/*` or `pkg:golang/*` components present.
2. **Given** the same fixture, **When** the operator runs `waybill sbom scan --path <root>` (default `--project-discovery=all`), **Then** the emitted SBOM contains 3 main-modules (`pkg:cargo/*`, `pkg:npm/*`, `pkg:golang/*`) plus all three ecosystems' transitive deps. Byte-identical to alpha.68 output.
3. **Given** a fixture with NO root-level manifest but ONLY nested manifests (`services/api/package.json`, `services/worker/go.mod`), **When** the operator runs `waybill sbom scan --path <root> --project-discovery=root-only`, **Then** waybill emits a WARN log naming the missing-root-level-manifest situation + falls back to the existing single-SBOM emission with synthetic-`pkg:generic/<repo>` root and ZERO main-modules. Preserves the "shallow means shallow" contract even when the root itself has no project file.

---

### User Story 2 - Respect ecosystem-native workspace-member declarations at the root level (Priority: P1)

An operator scans a Cargo workspace: root `Cargo.toml` has `[workspace] members = ["crates/*"]` and each subdir under `crates/` is a workspace member (`crates/a/Cargo.toml`, `crates/b/Cargo.toml`, etc.). Under `--project-discovery=root-only`, the operator STILL wants the workspace's declared members to be walked (their component-set flows into the SBOM as workspace-tagged siblings of the root) — because that's what a "Cargo workspace" means as a project. But the operator does NOT want an unrelated nested `bench/go.mod` (not declared as a workspace member) to be discovered. The distinction is: **workspace-declared members are part of the root project; independent nested projects are not.**

**Why this priority**: This is the load-bearing correctness constraint on US1. Without it, `--project-discovery=root-only` would break every Cargo workspace / npm workspaces / Go workspaces / Maven multi-module scan by silently dropping the workspace members. Real-world monorepos overwhelmingly use ecosystem-native workspace mechanisms — the flag would be useless if it fought those.

**Independent Test**: Author a fixture with a root `Cargo.toml` (declaring `[workspace] members = ["crates/api", "crates/worker"]`) + a nested independent `Gemfile` at `bench/Gemfile` (NOT declared as a workspace member of anything). Run `waybill sbom scan --path <root> --project-discovery=root-only`. Assert emitted SBOM contains: (a) the workspace root main-module, (b) both workspace-member components tagged with `waybill:workspace-member`, (c) all cargo transitive deps of both members. Assert: NO `pkg:gem/*` components (the Gemfile is not a declared member).

**Acceptance Scenarios**:

1. **Given** a Cargo workspace at scan root with declared members `crates/api` + `crates/worker`, plus an independent `bench/Gemfile`, **When** the operator runs `--project-discovery=root-only`, **Then** the emitted SBOM contains the workspace root as main-module, both crates as workspace-member components with `waybill:workspace-member` annotations, cargo transitive deps of both — AND ZERO `pkg:gem/*` components.
2. **Given** an npm workspaces root (root `package.json` with `"workspaces": ["packages/*"]`) plus an independent nested `services/api/Cargo.toml`, **When** the operator runs `--project-discovery=root-only`, **Then** the SBOM contains the npm workspace-root main-module + every declared `packages/*` member's `pkg:npm/*` components — AND ZERO `pkg:cargo/*` components.
3. **Given** a Go workspaces root (`go.work` at scan root + `use ("./api" "./worker")`) plus an independent nested `frontend/package.json`, **When** the operator runs `--project-discovery=root-only`, **Then** the SBOM contains the Go workspace's declared modules — AND ZERO `pkg:npm/*` components.

---

### User Story 3 - Strict-atomic mode for treating the workspace root as one file (Priority: P3)

Some operators want the truly-literal shallow interpretation: "scan ONLY the one file at scan-root; ignore even ecosystem-native workspace members." This is rarer (research tool audits, minimal-surface CI checks, contract-compliance snapshots keyed on a single manifest's declared deps), but a viable mode to expose. `--project-discovery=strict` selects this: no discovery below scan-root AT ALL, even for workspace-declared members.

**Why this priority**: Real but niche. Not the primary use case; ships alongside root-only rather than blocking on it. Extensibility precedent: matches the m219 `--split=<mode>` shape where mode variants cover progressively-narrower semantics.

**Independent Test**: Same Cargo-workspace fixture as US2. Run `waybill sbom scan --path <root> --project-discovery=strict`. Assert the SBOM contains ONLY the workspace-root's own components (not its declared workspace members). Compare against `--project-discovery=root-only` which DOES include the members.

**Acceptance Scenarios**:

1. **Given** the US2 Cargo workspace fixture, **When** the operator runs `--project-discovery=strict`, **Then** the SBOM contains the workspace root's own PURL + its own directly-declared deps ONLY. NO workspace-member components.
2. **Given** an m216 Gemfile-only Ruby app at scan root, **When** the operator runs `--project-discovery=strict`, **Then** the SBOM is identical to `--project-discovery=root-only` (Gemfile doesn't declare workspace members — no delta).

---

### Edge Cases

- **No manifest at scan root** (`--project-discovery=root-only` on a bare dir): FR-004 fallback — WARN log + single-SBOM emission with synthetic-`pkg:generic/<repo>` root + zero main-modules. Same shape m215 uses when `enumerate_workspace_roots` returns empty.
- **Manifest at scan root declares members that themselves declare members** (nested Cargo workspace: root workspace has `crates/inner-workspace` which is ALSO a `[workspace]` with its own members): FR-005 — root-only follows workspace-member declarations transitively (that's how Cargo semantics work: an inner workspace's members are still members of the root scan). Strict mode ignores all of it.
- **Ambiguous workspace-member declaration** (root `pyproject.toml` with `[tool.hatch.metadata.hooks.fancy-pypi-readme]` referencing nested packages — is that a workspace declaration? — poetry's `[tool.poetry.packages]` similarly): FR-006 — each ecosystem-specific reader's existing workspace-member-detection logic decides. m220 does NOT invent new detection heuristics; it consumes whatever the readers already do.
- **Mixed root** (root has BOTH a `Cargo.toml` AND a `package.json` — root-level polyglot, e.g., a Rails app with npm asset pipeline): root-only mode discovers BOTH root-level manifests as main-modules (both ARE at scan root; not nested). Result: 2 root-level main-modules in the SBOM. Strict mode is the same (both files are at scan root).
- **Empty scan root**: no manifests anywhere → existing empty-scan behavior (no components; SBOM with just metadata). `--project-discovery` value has no effect.
- **Interaction with `--split=directory`**: root-only + split=directory yields one sub-SBOM (there's only one directory group: the root). The FR-009 fallback (WARN + single SBOM) fires cleanly because effectively there's one "workspace boundary."
- **Interaction with `--split=workspace`**: root-only + split=workspace yields one sub-SBOM per root-level main-module (typically 1 for single-project scans, 2 for polyglot roots).
- **Interaction with `--exclude-path`**: exclude-path applies FIRST (walker-level), then project-discovery scope filters main-module tagging. Layered independently.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CLI MUST accept a new flag `--project-discovery=<mode>` accepting values `all` (default; current behavior), `root-only` (new; discover only root-level main-modules + their ecosystem-native workspace members), and `strict` (new; discover ONLY the root-level manifest itself, no workspace-member walking).
- **FR-002**: `--project-discovery=all` MUST produce output byte-identical to alpha.68 default behavior on every existing test fixture (SC-005 byte-identity gate — no golden regeneration required for this mode).
- **FR-003**: `--project-discovery=root-only` MUST restrict main-module DISCOVERY to manifests located directly at the scan-root path (depth-0). Nested independent manifests (not declared as workspace members by any root-level manifest) MUST NOT contribute components, main-modules, or transitive deps to the emitted SBOM.
- **FR-004**: `--project-discovery=root-only` MUST preserve ecosystem-native workspace-declared members in the emitted SBOM. Members are identified via the existing `waybill:workspace-member` annotation set by per-ecosystem readers today (m127-era; Cargo `[workspace] members`, npm `"workspaces"`, Go `go.work use`, Maven `<modules>` — see FR-006). **No per-reader walker changes are required**: readers walk everything at discovery time under all modes; m220's post-discovery filter RETAINS components whose `waybill:workspace-member` annotation value matches an in-scope root's PURL and DROPS non-annotated nested-independent components. Members' `pkg:*` components + transitive deps therefore land in the SBOM tagged with `waybill:workspace-member` (unchanged from today's shape).
- **FR-005**: `--project-discovery=root-only` MUST transitively follow workspace-member declarations. When a workspace member IS ITSELF a workspace (Cargo permits nested workspaces via inherited config), that inner workspace's members MUST also be walked. The recursion terminates when a member declares no further workspace members.
- **FR-006**: The per-ecosystem workspace-member detection logic MUST be the SAME logic those readers already use to distinguish "workspace member" from "independent project" today. m220 does NOT invent new detection heuristics. When a reader's workspace-member semantics are ambiguous (e.g., pyproject.toml package-declaration variants), m220 defers to the reader's existing decision — root-only mode inherits whatever the reader considers a "member."
- **FR-007**: `--project-discovery=strict` MUST discover ONLY the root-level manifest file(s). Workspace-member declarations are IGNORED — the SBOM contains the root manifest's own PURL + directly-declared deps ONLY. No workspace-member walking.
- **FR-008**: When a scan under `--project-discovery=root-only` or `=strict` finds ZERO root-level manifests, waybill MUST emit a WARN log naming the mode + the empty-root situation, then fall back to existing single-SBOM emission with a synthetic `pkg:generic/<repo>@0.0.0-unknown` root. Existing empty-scan behavior preserved for the non-manifest-at-root case.
- **FR-009**: The flag MUST interact cleanly with `--split[=<mode>]`: `--split=workspace` + `--project-discovery=root-only` yields one sub-SBOM per root-level main-module (typically 1); `--split=directory` + `--project-discovery=root-only` yields one sub-SBOM for the root directory. No new "sub-mode" combinations required — the two flags compose orthogonally.
- **FR-010**: An invalid `--project-discovery=<value>` MUST cause CLI parse to fail with a non-zero exit and a stderr error naming the three accepted values. Empty string, whitespace-only, uppercase mode names are all invalid.
- **FR-011**: A new document-scope annotation `waybill:project-discovery-mode` MUST be emitted on every SBOM output whose scan used a non-default mode. Value is one of `"root-only"`, `"strict"`. Absent from SBOMs scanned with the default `all` mode (byte-identity preserved). Enables downstream consumers to detect that the SBOM represents a scoped view.
- **FR-012**: The FR-013-style INFO log emitted at scan-driver exit MUST include the mode string when non-default. Format: `INFO scan: project-discovery=<mode> root_main_modules=<N> workspace_members_followed=<M> nested_projects_ignored=<K>`. `M` counts workspace-members walked via ecosystem-native declarations; `K` counts main-modules that WOULD have been discovered under `all` but were ignored under the current mode. The `K` counter is the operator-visible signal of "how much did the scope cap actually change."
- **FR-013**: Documentation at `docs/reference/project-discovery.md` (NEW page) MUST cover: (a) the three mode values with when-to-choose guidance; (b) an interaction matrix vs `--split[=<mode>]`; (c) per-ecosystem workspace-member detection rules (which readers recognize which declaration shapes); (d) worked examples for a Cargo workspace + a polyglot monorepo + an m216 Gemfile-only Ruby app; (e) the extensibility contract for future modes.

### Key Entities *(include if feature involves data)*

- **ProjectDiscoveryMode**: enum with variants `All` (default; current behavior), `RootOnly` (new; discover only root-level main-modules + ecosystem-native workspace members), `Strict` (new; discover only the root-level manifest itself). Extensibility contract mirrors m219 `SplitMode` — future variants (`explicit=<paths>`, `depth=<N>`) plug in via a `discovery_scope(&scan_root, &manifest_path) -> ShouldDiscover` method on the enum.
- **Root-level manifest**: a manifest file whose parent directory equals the scan-root path canonically. Determined via `std::fs::canonicalize(manifest.parent()) == std::fs::canonicalize(scan_root)`.
- **Workspace-member declaration**: an ecosystem-specific mechanism by which a root-level manifest names other directories as members of the same logical project. Recognized shapes: Cargo `[workspace] members = [...]`, npm `"workspaces": [...]`, Go `go.work use (...)`, Maven `<modules>...</modules>`. Per-ecosystem detection logic is the reader's existing behavior (m220 does not extend it).
- **Nested independent project**: a manifest file located in a subdirectory of scan-root whose parent is NOT declared as a workspace member by any root-level manifest. Under `--project-discovery=root-only` these are IGNORED at discovery time (walker doesn't open them; their components + deps never enter the SBOM).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a polyglot fixture with root `Cargo.toml` + nested `services/api/package.json` + `services/worker/go.mod`, `waybill sbom scan --path <root> --project-discovery=root-only` emits an SBOM whose `components[]` contains ONLY the root cargo main-module + its cargo transitive deps. Verified via `jq '.components[] | .purl' | grep -cE "^pkg:(npm|golang)"` returning `0`.
- **SC-002**: On the same fixture, `--project-discovery=all` emits an SBOM with 3 main-modules (cargo + npm + golang). Verified via jq main-module count.
- **SC-003**: On a Cargo workspace fixture (root `[workspace] members = ["crates/api", "crates/worker"]`), `--project-discovery=root-only` emits an SBOM containing the workspace-root main-module + both workspace-members tagged with `waybill:workspace-member` + all their cargo transitive deps. Verified via jq workspace-member component count == 2.
- **SC-004**: On the same Cargo workspace fixture PLUS an independent `bench/Gemfile` (not declared as workspace member), `--project-discovery=root-only` emits an SBOM with ZERO `pkg:gem/*` components. Verified via jq `pkg:gem/` count == 0.
- **SC-005**: Byte-identity: `--project-discovery=all` (or flag omitted entirely) produces byte-identical output to alpha.68 on every existing test fixture. Zero goldens regenerate. Enforced by keeping the flag opt-in (defaults to `all`).
- **SC-006**: `--project-discovery=strict` on the US2 Cargo workspace fixture yields an SBOM with the root workspace's own PURL + directly-declared deps ONLY. Workspace members ABSENT. Verified via jq workspace-member component count == 0.
- **SC-007**: Invalid mode value: `waybill sbom scan --path <root> --project-discovery=nonexistent-mode` exits non-zero, stderr contains the three accepted values (`all`, `root-only`, `strict`), zero output files created.
- **SC-008**: FR-011 doc-scope annotation `waybill:project-discovery-mode` is present iff the scan used non-default mode. Verified via jq `metadata.properties[]` containing the annotation for `--project-discovery=root-only` scans + absent for default scans.
- **SC-009**: FR-012 INFO log emitted at scan-driver exit contains `project-discovery=root-only` substring under `--project-discovery=root-only` invocations. Verified via `RUST_LOG=info` capture in an integration test.
- **SC-010**: FR-013 documentation page exists at `docs/reference/project-discovery.md`, is linked from README + `docs/index.md`, and covers all five required topics (verified via lint-step grep).
- **SC-011**: The `--project-discovery` flag composes cleanly with `--split=<mode>`: `--project-discovery=root-only --split=directory` on a nested-projects fixture yields exactly ONE sub-SBOM (the root's directory group). Verified via `ls <output-dir>/*.cdx.json | wc -l == 1`.
- **SC-012**: FR-005 nested-workspace-following: a fixture with root workspace whose member IS ITSELF a workspace produces a `waybill:workspace-member` annotation on every level of the recursion. Verified via jq counting members at depth-1 + depth-2.

## Assumptions

- The primary use case is polyglot monorepo scans where an operator wants to scope to one project + its ecosystem-native workspace members without pulling in unrelated nested projects. Real-world shapes: dev containers with per-team subdirs, CI matrix scans, contract-compliance snapshots.
- Default value `--project-discovery=all` preserves byte-identity with alpha.68 output. Zero goldens regenerate. SC-005 is the load-bearing invariant.
- The three modes cover 80-90% of the design space per user discussion (2026-07-24). Per-ecosystem selective scope (e.g., "shallow for npm but honor workspaces for cargo") is deferred to a future config-file-shaped milestone — not in scope for m220.
- CLI flag shape `--project-discovery=<mode>` with `clap::ValueEnum` derive matches the m219 `--split=<mode>` precedent. Extensibility via enum variants + method (per m219 contracts/grouping-strategy.md pattern) supports future variants (`explicit=<paths>`, `depth=<N>`) without CLI-flag rewrites.
- Per-ecosystem workspace-member detection logic is REUSED from existing readers. m220 does NOT invent new detection heuristics or extend readers' understanding of what constitutes a workspace member. Ambiguous cases (pyproject.toml package-declaration variants, ruby's lack of workspace concept, etc.) inherit whatever the reader currently decides.
- Interaction with `--split[=<mode>]`: orthogonal. m220's scope cap runs at MAIN-MODULE-DISCOVERY time (before enumerate_workspace_roots outputs its Vec<SubprojectRoot>); m219's split-mode grouping runs POST-discovery. Composition is natural: fewer discovered main-modules → fewer/smaller sub-SBOMs.
- FR-011 doc-scope annotation `waybill:project-discovery-mode` follows the m216 C135 + m217 C136 + m219 (silence-on-default) precedent: present when non-default; absent from default-mode SBOMs to preserve byte-identity.
- Documentation lives at `docs/reference/project-discovery.md` (NEW page) matching the m218/m219 pattern.
- Non-goals: (1) inventing new per-ecosystem workspace-member detection logic; (2) supporting per-ecosystem selective scope in this milestone (config-file territory — future); (3) exposing depth-cap semantics like `--depth=N` (a different design question about walker depth, not project discovery); (4) changing byte-identity of default-mode output.
- Not blocking: no bug open in the tracker directly asks for this. Motivated by 2026-07-24 user discussion driven by real polyglot monorepo scan pain.
