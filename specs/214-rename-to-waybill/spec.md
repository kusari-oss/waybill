# Feature Specification: Rename mikebom → waybill across all functional identifiers

**Feature Branch**: `214-rename-to-waybill`
**Created**: 2026-07-21
**Status**: Draft
**Input**: User description: "I am looking to rename EVERYTHING in Mikebom to the new name Waybill. We can keep historical stuff like: Waybill (formerly Mikebom) but anything functional including mikebom annotations should end up with the name waybill. No functionality change should happen in this change outside of that."

## Clarifications

### Session 2026-07-21

- Q: Wire-shape back-compat window for `mikebom:*` annotations — hard break, one-alpha bridge, or bridge until v1.0? → A: Hard break — first post-rename release emits only `waybill:*` annotations. No dual-emit / no transition period. Downstream tooling does a mechanical `s/mikebom:/waybill:/g` at their input layer per the migration guide (FR-015).
- Q: Version-bump semantic for the first post-rename release? → A: `0.1.0-alpha.66` (sequential alpha bump). Release-notes + PR body carry a prominent "BREAKING" callout linking to the migration guide. First alpha.66 ships as **waybill** — binary name `waybill`, release artifacts `waybill-v0.1.0-alpha.66-*`, Docker image `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66`. No workflow changes required (auto-tag-release.yml + release.yml already gate on the `v*-alpha.*` pattern).
- Q: Any workspace crates currently published to crates.io? → A: No — confirmed. No `cargo publish` in release.yml; no `mikebom-*` entries on crates.io. Scope stays constrained to local + Docker + GHCR + binary artifacts; no crates.io deprecation steps needed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Downstream SBOM consumer parses annotations under the new prefix (Priority: P1)

A downstream security scanner or compliance tool that ingests SBOMs produced by this project parses the tool-specific annotations to extract provenance data (build-inclusion decisions, source-mechanism markers, filter-category aggregates, etc.). Post-rename, every one of those annotation keys uses the `waybill:` prefix instead of `mikebom:`. The consumer updates their parser once (single-string search-and-replace at the prefix level) and continues to work.

**Why this priority**: This is the biggest cross-boundary impact of the rename. The 192 distinct `mikebom:*` annotation names are the primary wire-shape contract with the world. Getting this consistent (all-or-nothing under one prefix in each emitted SBOM) is the single most important correctness gate — a mixed-prefix output would silently break downstream tooling that expects a stable prefix.

**Independent Test**: Generate an SBOM in each of the three supported formats (CycloneDX 1.6 JSON, SPDX 2.3 JSON, SPDX 3.0.1 JSON) on any fixture and grep for `mikebom:` — the count MUST be zero. Grep for `waybill:` — the count MUST equal what the pre-rename SBOM had under `mikebom:`.

**Acceptance Scenarios**:

1. **Given** a post-rename SBOM emitted for any fixture, **When** a consumer parses annotation keys, **Then** every tool-specific annotation key starts with `waybill:` and none start with `mikebom:`.
2. **Given** a downstream tool's parser code that previously handled `mikebom:*` keys, **When** the tool updates its prefix constant from `"mikebom:"` to `"waybill:"`, **Then** it correctly parses all 192 annotation types from the new SBOMs without further code changes.
3. **Given** any pair of "same fixture, pre-rename SBOM" and "same fixture, post-rename SBOM", **When** an operator compares the two files byte-for-byte after applying a `sed 's/mikebom/waybill/g' + tool-metadata-version` normalization, **Then** the semantic content is byte-identical (no fields dropped, no fields added, no reordering).

---

### User Story 2 - Operator invokes the renamed CLI + env vars (Priority: P1)

A developer or CI script invokes the tool via its CLI. Post-rename, they type `waybill <noun> <verb>` instead of `mikebom <noun> <verb>`, and set environment variables like `WAYBILL_FIXTURES_DIR` instead of `MIKEBOM_FIXTURES_DIR`. Subcommand structure, argument shapes, exit codes, and output formats are all unchanged — only the binary name and env-var prefix.

**Why this priority**: The CLI is the primary user-facing surface. Any lag between the crate rename and the binary rename would leave the tool half-renamed. Same for env vars — a mixed state (some `MIKEBOM_*`, some `WAYBILL_*`) is worse than either extreme.

**Independent Test**: Install the post-rename release, run `waybill --version` — succeeds and prints `waybill v0.1.0-alpha.X`. Run `mikebom --version` — should fail with "command not found" (unless the operator explicitly aliased). Set `WAYBILL_LOG=debug waybill sbom scan --path .` on a sample project and confirm normal operation.

**Acceptance Scenarios**:

1. **Given** the post-rename release installed, **When** an operator runs `waybill --help`, **Then** it prints usage text identical in structure to the pre-rename `mikebom --help` but with `waybill` substituted throughout.
2. **Given** an operator's pre-rename CI script that sets `MIKEBOM_LOG=info` + calls `mikebom sbom scan`, **When** they replace the two references with `WAYBILL_LOG=info` + `waybill sbom scan`, **Then** their script works identically to before.
3. **Given** a post-rename container image `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.X`, **When** an operator pulls and runs it, **Then** the entrypoint is `waybill` (not `mikebom`) and all functionality matches the pre-rename image.

---

### User Story 3 - New contributor onboards to the renamed codebase (Priority: P2)

A developer reading the repository for the first time encounters `waybill-cli`, `waybill-common`, `waybill-ebpf` as crate names in the workspace `Cargo.toml`; sees `use waybill_common::foo` in imports; opens the README and sees the project introduced as "Waybill" (with an optional historical footnote "formerly Mikebom"); reads the constitution and sees "Waybill Constitution" at the top; reads any spec doc under `specs/` and sees consistent `waybill` references in prose. They can search the codebase for `mikebom` and find only preserved-heritage attributions (README-style footnotes, changelog entries, commit-message references to old milestones) plus git-history commits — zero functional references.

**Why this priority**: Contributor experience matters for a project's health, but doesn't gate any user-facing release. Cleaning up prose + docs can happen in the same rename PR without adding significant complexity.

**Independent Test**: `grep -rE '\bmikebom\b' .` on the post-rename tree; every hit MUST fall into one of these permitted classes: (a) `specs/*/*.md` historical spec docs whose milestone predates the rename, (b) README/CHANGELOG heritage-attribution sentences containing "formerly Mikebom" or equivalent, (c) commit messages / git-log output that git itself preserves, (d) test fixtures whose external inputs contain the string mikebom for reasons unrelated to this project's identifiers. All other hits are rename bugs.

**Acceptance Scenarios**:

1. **Given** a fresh clone of the post-rename repo, **When** a contributor runs `cargo build --workspace`, **Then** it succeeds and produces a `target/release/waybill` binary (no `mikebom` binary artifact).
2. **Given** the same clone, **When** the contributor reads the top of `README.md`, **Then** the project name appears as "Waybill" with an optional single-sentence heritage note.
3. **Given** the same clone, **When** the contributor `grep -rE '\bmikebom\b' Cargo.toml **/Cargo.toml src/`, **Then** zero matches are returned.

---

### User Story 4 - Existing users find migration guidance (Priority: P3)

A pre-rename user of the tool (someone running mikebom v0.1.0-alpha.64 or v0.1.0-alpha.65) updates to the first post-rename release. They read the release notes / migration guide, understand which of their scripts / CI configs / downstream tools need updating (binary name, env vars, annotation prefix), and complete the migration.

**Why this priority**: Existing users are a small population (alpha software), and the migration is mechanical. But documenting the mapping explicitly reduces friction and support burden.

**Independent Test**: A migration guide document exists at a predictable path (e.g., `docs/migration/mikebom-to-waybill.md` or in the release-notes body). It lists: (a) binary rename (`mikebom` → `waybill`), (b) env-var prefix rename (all 30+ variables), (c) annotation prefix rename (`mikebom:*` → `waybill:*` — the mapping is a pure prefix swap; no other structural changes), (d) Docker image rename, (e) any other cross-cutting rename points.

**Acceptance Scenarios**:

1. **Given** a pre-rename user reading the post-rename release notes, **When** they follow the linked migration guide, **Then** they can complete their migration by executing a documented mechanical text-substitution across their configs + scripts.
2. **Given** the migration guide, **When** a user searches for a specific pre-rename identifier (e.g., `MIKEBOM_HELM_RENDER_TIMEOUT_SECS`), **Then** the guide either lists it explicitly or documents the general pattern (`MIKEBOM_* → WAYBILL_*`) that covers it.

---

### Edge Cases

- **Golden test fixtures**: All golden CDX / SPDX-2.3 / SPDX-3 JSON files under `tests/fixtures/golden/` embed the `mikebom:*` annotation prefix in the tool-metadata section. These will be regenerated via the existing `MIKEBOM_UPDATE_*_GOLDENS=1` env-var pattern (which itself gets renamed to `WAYBILL_UPDATE_*_GOLDENS`). Regeneration is a mechanical `cargo test` invocation; no manual editing.
- **Docker image / GHCR path**: `ghcr.io/kusari-oss/mikebom` → `ghcr.io/kusari-oss/waybill`. The GitHub repo has already been renamed to `waybill`, so the GHCR namespace already aligns. Pre-rename image tags at `ghcr.io/kusari-oss/mikebom:v*-alpha.64` remain available as historical artifacts; new pushes go to `ghcr.io/kusari-oss/waybill:v*-alpha.X`.
- **Downstream SBOM consumers with cached `mikebom:*` parsers**: These break the moment they encounter a post-rename SBOM. Mitigation: prominent release-notes flag + migration guide (US4). This is an alpha software project so hard-break is acceptable per user direction.
- **Local workspace directory**: `/Users/mlieberman/Projects/mikebom` on developer machines — not spec-scope. Developers rename their local checkouts at their own discretion; CI + tooling references use relative paths.
- **Historical spec docs**: `specs/001-*` through `specs/213-*` reference `mikebom` throughout their prose. Preserving them as-is (as historical artifacts of milestones authored under the old name) is the default; only `specs/214-*` (this spec) and onward use `waybill` in prose.
- **Constitution file**: `.specify/memory/constitution.md` starts with "# mikebom Constitution". Post-rename: "# Waybill Constitution", with a one-line heritage note in the preamble ("Waybill was previously known as mikebom; historical version notes retain their original terminology.").
- **Cargo package publication**: If any of these crates are published to crates.io (`waybill-cli`, `waybill-common`), the package names change. Pre-rename `mikebom-*` crates would remain on crates.io as historical artifacts. If crates are NOT published to crates.io currently, this is a no-op — only local + workspace + Docker naming matters.
- **eBPF binary name**: `waybill-ebpf` compiles to `target/bpfel-unknown-none/release/waybill-ebpf`. The loader at `waybill-cli/src/trace/loader.rs` needs its `default_ebpf_path` updated to look for the renamed artifact.

## Requirements *(mandatory)*

### Functional Requirements

**Identifier rename (functional surface)**:

- **FR-001**: The primary binary MUST be named `waybill` (was `mikebom`).
- **FR-002**: The three workspace crates MUST be named `waybill-cli`, `waybill-common`, `waybill-ebpf` (were `mikebom-cli`, `mikebom-common`, `mikebom-ebpf`).
- **FR-003**: Rust module paths visible to callers MUST use snake_case forms `waybill_cli`, `waybill_common`, `waybill_ebpf` (were `mikebom_cli`, `mikebom_common`, `mikebom_ebpf`).
- **FR-004**: All CLI environment variables MUST use the `WAYBILL_` prefix (were `MIKEBOM_`). This applies to every variable that currently begins with `MIKEBOM_` — no exceptions.
- **FR-005**: Every tool-specific annotation key emitted in CycloneDX 1.6, SPDX 2.3, and SPDX 3.0.1 SBOMs MUST use the `waybill:` prefix (was `mikebom:`). The 192 distinct annotation names (per pre-rename scope survey) are renamed as a mechanical prefix swap: the portion after the colon is unchanged.
- **FR-006**: The published Docker image MUST be at `ghcr.io/kusari-oss/waybill:v*-alpha.*` (was `ghcr.io/kusari-oss/mikebom:v*-alpha.*`). Pre-rename image tags remain untouched as historical artifacts.
- **FR-007**: The eBPF kernel-side binary path MUST be `target/bpfel-unknown-none/release/waybill-ebpf` (was `.../mikebom-ebpf`), and the userspace loader's default lookup path MUST match.
- **FR-008**: The generator-tool metadata embedded in every emitted SBOM (CDX `metadata.tools[]`, SPDX 2.3 `creationInfo.creators`, SPDX 3 creator entities) MUST identify the tool as `waybill` (was `mikebom`).

**Documentation + prose rename**:

- **FR-009**: `README.md` MUST refer to the project as "Waybill" in all functional passages (project introduction, badges, installation instructions, usage examples). A single heritage sentence like "Waybill was previously known as Mikebom" is permitted and encouraged; multiple references to the old name in different sentences are discouraged.
- **FR-010**: `.specify/memory/constitution.md` MUST title as "Waybill Constitution" and use "Waybill" in all normative principle text. A one-line heritage preamble is permitted.
- **FR-011**: `CLAUDE.md`, `docs/**/*.md`, and any other authored documentation MUST use "Waybill" as the primary project name in prose. Historical references (e.g., a changelog entry saying "In alpha.64 we added feature X to mikebom") MAY be preserved verbatim as historical artifacts if the entry itself predates this rename.
- **FR-012**: Historical spec documents at `specs/001-*/` through `specs/213-*/` MUST be preserved unchanged as historical artifacts. Only `specs/214-*/` (this spec) and subsequent specs use the new name in prose.

**Wire-shape + versioning**:

- **FR-013**: The first post-rename release MUST be flagged as a wire-shape breaking release in its release notes and version-bump PR body. Downstream SBOM consumers parsing `mikebom:*` annotations WILL need to migrate.
- **FR-014**: The wire-shape rename is a mechanical prefix substitution: every `mikebom:<x>` becomes `waybill:<x>` with no other structural changes to the JSON. Downstream parsers doing a single-string prefix search-and-replace at their input-parsing layer will handle every annotation.
- **FR-015**: A migration guide document MUST exist at `docs/migration/mikebom-to-waybill.md` (or equivalent) documenting: (a) binary rename, (b) env-var prefix rename with an explicit list of all `MIKEBOM_*` → `WAYBILL_*` mappings, (c) annotation prefix rename with a link to the exhaustive list, (d) Docker image rename.

**Test infrastructure**:

- **FR-016**: All existing golden test files under `tests/fixtures/golden/` MUST be regenerated post-rename via the existing `WAYBILL_UPDATE_*_GOLDENS=1` (renamed from `MIKEBOM_UPDATE_*_GOLDENS`) env-var pattern. Regeneration MUST NOT introduce any diff other than: (a) the prefix substitution `mikebom → waybill` at every occurrence, (b) tool-metadata version-string bumps for the release itself.
- **FR-017**: `./scripts/pre-pr.sh` and the CI matrix (`cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace`) MUST pass on the post-rename tree with zero test failures.

**Scope constraints (out-of-scope)**:

- **FR-018**: No functional behavior change is permitted in this rename. No refactoring, no dep bumps, no feature additions, no bug fixes are bundled with this PR. If a bug is discovered during the rename, it is filed as a separate follow-up issue and NOT fixed inline.
- **FR-019**: No changes to the CLI subcommand structure, argument names, argument types, exit codes, or output formats are permitted beyond the binary-name change (`mikebom` → `waybill`).
- **FR-020**: Wire-shape semantics beyond the prefix rename MUST be preserved byte-identical. Field ordering, JSON key ordering (where deterministic), NUL padding, all preserved.

### Key Entities

- **Functional identifier**: any string, symbol, path, or name that participates in program execution or emitted output — binary name, crate name, env-var name, annotation key, module path, file path used at runtime, Docker image name, tool-metadata identifier. In scope for rename.
- **Historical artifact**: any string, symbol, path, or name that documents past state or attribution — spec docs authored under the old name, changelog entries mentioning old milestones, commit messages, git tag names of prior releases, "formerly known as" attribution text. Out of scope for rename; preserved as-is.
- **Wire-shape contract**: the JSON structure of emitted SBOMs (CDX, SPDX-2.3, SPDX-3) as parsed by downstream consumers. The `mikebom:*` annotation prefix is part of this contract; renaming it to `waybill:*` is definitionally a wire-shape change that consumers must adapt to.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `grep -rE '\bmikebom\b' src/ mikebom-cli/src/ mikebom-common/src/ mikebom-ebpf/src/ xtask/src/` post-rename returns **zero matches** (verify via CI job). Any occurrence in source code is a rename bug.
- **SC-002**: `grep -rE '\bmikebom-cli\b|\bmikebom-common\b|\bmikebom-ebpf\b|\bmikebom_cli\b|\bmikebom_common\b|\bmikebom_ebpf\b' Cargo.toml **/Cargo.toml` post-rename returns **zero matches**.
- **SC-003**: Grep for the annotation prefix in emitted SBOMs on any fixture: `mikebom:` count is 0; `waybill:` count is >0 (specifically: the count matches the annotation-count that was present under `mikebom:` in the equivalent pre-rename SBOM).
- **SC-004**: `cargo +stable clippy --workspace --all-targets -- -D warnings` passes clean on the post-rename tree.
- **SC-005**: `cargo +stable test --workspace` passes with **every suite reporting `ok. N passed; 0 failed`** on the post-rename tree.
- **SC-006**: The post-rename release-notes / migration guide is complete and covers all 30+ renamed environment variables, the binary rename, and the annotation prefix rename with a link to the exhaustive list.
- **SC-007**: An operator following the migration guide can complete their end-to-end migration (env-var rename in scripts + annotation-prefix rename in downstream tooling + binary-name rename in CI invocations) using pure mechanical text substitution — no code review or judgment required.
- **SC-008**: The post-rename Docker image at `ghcr.io/kusari-oss/waybill:v*-alpha.<first-post-rename>` pulls cleanly and its entrypoint is `waybill`.

## Assumptions

- **Wire-shape hard break is acceptable**: The user has explicitly directed that "anything functional including mikebom annotations should end up with the name waybill" — this is a wire-shape change for downstream SBOM consumers, but the user's directive is that the annotation prefix rename IS part of the rename scope, not a separate compatibility concern. The rename ships as a **breaking release** for consumers of the annotation prefix, with a migration guide as mitigation. No dual-emit / bridge-release period is required. This is consistent with the project's alpha status.
- **Repo already renamed on GitHub**: `github.com/kusari-oss/mikebom` has already redirected to `github.com/kusari-oss/waybill`. This rename spec picks up from that state and completes the source-tree side.
- **Docker image namespace already aligned**: `ghcr.io/kusari-oss/waybill` is the natural target given the repo rename. Pre-rename image tags at `ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.7` through `v0.1.0-alpha.65` remain available as historical artifacts.
- **No crates.io publication currently** (confirmed 2026-07-21 clarification): No workspace crates are published to crates.io. Scope stays constrained to local build + Docker image + GHCR + release binaries. No `cargo publish` step exists in `release.yml`; no `waybill-cli` / `waybill-common` publication step is added by this rename.
- **Historical spec docs stay unchanged**: `specs/001-*` through `specs/213-*` are historical artifacts of milestones authored under the old name. Rewriting all of them would create massive git churn for zero user value. Only current+future specs use the new name in prose.
- **Golden regeneration cascades**: The 34+ golden test files will diff for the prefix-string rename via the same `MIKEBOM_UPDATE_*_GOLDENS` → `WAYBILL_UPDATE_*_GOLDENS` env-var pattern that release-bumps use today.
- **All CI matrix lanes must pass**: linux-x86_64 (default features + ebpf-tracing), macOS, Windows. Plus Kusari Inspector + rootfs/language scanners. Any one lane red = rename bug.
- **Pre-rename local workspace path is developer-local**: The user's local checkout is at `/Users/mlieberman/Projects/mikebom`. The rename spec does not dictate a filesystem rename — developers may rename their local dirs at their own discretion. All tooling uses relative paths.
- **`mikebom-ebpf` untouched code path**: Even though the mikebom-ebpf crate is renamed to `waybill-ebpf`, no functional change happens inside it. The rename touches: crate name in Cargo.toml, imports of `mikebom_common` → `waybill_common`, the output binary path.
- **Constitution version bump = MAJOR** (per constitution's own governance rules): The constitution changes its own project name and multiple principle titles. This is a MAJOR bump under the amendment procedure documented in the constitution itself (2.x → 3.0.0). The bump PR must include a SYNC IMPACT REPORT block per the constitution's convention.
