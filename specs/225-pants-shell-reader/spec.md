# Feature Specification: Pants shell reader

**Feature Branch**: `225-pants-shell-reader`
**Created**: 2026-08-02
**Status**: Draft
**Input**: User description: "pants shell reader"

## User Scenarios & Testing *(mandatory)*

Pants monorepos routinely include shell scripts as first-class build
artifacts (deployment scripts, wrappers, test helpers) declared via
the Pants `shell` backend's BUILD-file target types (`shell_source`,
`shell_sources`, `shunit2_test`, `shunit2_tests`). These scripts are
part of the software supply chain — they run in CI, they ship to
production, they influence releases — but every existing waybill
reader today misses them because shell has no lockfile analogue to
Pex or coursier.

The Pants shell backend also pins its lint and test tooling
(`shellcheck`, `shfmt`, `shunit2`) via `pants.toml` subsystem
sections. Those pinned versions are load-bearing supply-chain data:
"which shellcheck version audited this repo's scripts" is a real
question compliance teams ask.

This feature adds a Pants-aware shell reader that ingests both:
BUILD-file-declared shell scripts (file-tier components with SHA-256
fingerprints and Pants target-address annotations) AND
pants.toml-pinned shell tooling (design-tier `pkg:generic` build-tool
components).

### User Story 1 - Discover BUILD-declared shell scripts (Priority: P1) 🎯 MVP

An operator scans a Pants monorepo. Every shell script declared by a
`shell_source` or `shell_sources` target under any `BUILD` file in
the scan root appears in the emitted SBOM as a file-tier
`pkg:generic/*` component with a SHA-256 fingerprint and a
`waybill:pants-target=<address>` annotation naming the target that
owns it.

**Why this priority**: Establishes the first Pants BUILD-file walker
in waybill (m223 used the Pex lockfile; m224 used the coursier
lockfile; neither walked BUILD files). Delivers immediate value:
shell-script inventory across the monorepo, which no other reader
today produces. Also lays down the walker infrastructure for
future features (Pants Go BUILD walker, Pants Docker, etc.).

**Independent Test**: Given a Pants repo containing
`scripts/BUILD` with `shell_sources(name="build", sources=["*.sh"])`
and two `.sh` files in that directory, `waybill sbom scan` emits
exactly two `pkg:generic/*` components, each with a SHA-256 hash and
a `waybill:pants-target=scripts:build` annotation.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `scripts/BUILD` declaring
   `shell_source(name="deploy", source="deploy.sh")` and
   `scripts/deploy.sh` present, **When** operator runs
   `waybill sbom scan`, **Then** the emitted SBOM contains a
   `pkg:generic/*` component representing `scripts/deploy.sh` with a
   SHA-256 hash matching the file's content and a
   `waybill:pants-target=scripts:deploy` annotation.
2. **Given** a Pants repo with `helpers/BUILD` declaring
   `shell_sources(name="utils", sources=["*.sh"])` and three
   `.sh` files in that directory, **When** operator runs
   `waybill sbom scan`, **Then** exactly three file-tier components
   are emitted (one per `.sh` file), each with its own SHA-256 hash
   and a shared `waybill:pants-target=helpers:utils` annotation.
3. **Given** a Pants repo whose `BUILD` file declares
   `shell_source(name="deploy", source="deploy.sh")` but
   `scripts/deploy.sh` does NOT exist on disk (declaration
   references a missing file), **When** operator runs
   `waybill sbom scan`, **Then** the scan does not abort; a WARN log
   names the missing target + file path; no component is emitted for
   the missing file.

---

### User Story 2 - Inventory pinned shell tooling (Priority: P2)

An operator's `pants.toml` includes
`[shellcheck] version = "v0.9.0"` and
`[shfmt] version = "v3.7.0"`. The emitted SBOM contains one
design-tier `pkg:generic/*` component per pinned tool, so downstream
compliance tooling can answer "what shellcheck version audited this
repo".

**Why this priority**: Second-order value — the tool inventory is
useful only if the user has already gained the primary script
inventory from US1. Also, some Pants repos don't pin tool versions
explicitly (they rely on Pants defaults), so this story's payoff is
smaller than US1's.

**Independent Test**: Given a Pants repo with `pants.toml`
containing `[shellcheck] version = "v0.9.0"`,
`waybill sbom scan` emits one `pkg:generic/shellcheck@v0.9.0`
component with `waybill:sbom-tier=design` and
`waybill:source-file=pants.toml` annotations.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `pants.toml` declaring
   `[shellcheck] version = "v0.9.0"` and
   `[shfmt] version = "v3.7.0"`, **When** operator runs
   `waybill sbom scan`, **Then** the emitted SBOM contains exactly
   two `pkg:generic/*` components (`shellcheck@v0.9.0` and
   `shfmt@v3.7.0`), each with a `waybill:sbom-tier=design`
   annotation.
2. **Given** a Pants repo with `pants.toml` present but no
   `[shellcheck]` / `[shfmt]` sections, **When** operator runs
   `waybill sbom scan`, **Then** no tool components are emitted by
   this reader (script inventory from US1 unaffected).
3. **Given** a Pants repo with `[shellcheck]` declared but no
   `version` key (relying on the Pants default), **When** operator
   runs `waybill sbom scan`, **Then** no `shellcheck` component is
   emitted (waybill declines to hardcode Pants's built-in defaults —
   only operator-pinned versions land in the SBOM).

---

### User Story 3 - Tag shunit2 tests with development scope (Priority: P3)

Files owned by `shunit2_test` / `shunit2_tests` targets get
`lifecycle_scope=Development` so that downstream security tooling
can filter test artifacts out of production dependency inventories.

**Why this priority**: Refinement, not primary value. Without US3,
operators still get script inventory (US1) — they just get it
without the test/non-test distinction that helps them focus
vulnerability triage on production scripts.

**Independent Test**: Given a Pants repo with
`shunit2_tests(name="unit", sources=["*_test.sh"])` and one
`foo_test.sh` file, `waybill sbom scan` emits a component for
`foo_test.sh` with `waybill:lifecycle-scope=development`.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `tests/BUILD` declaring
   `shunit2_test(name="deploy-test", source="deploy_test.sh")` and
   the file present, **When** operator runs `waybill sbom scan`,
   **Then** the emitted component carries
   `waybill:lifecycle-scope=development` AND
   `waybill:pants-target=tests:deploy-test`.
2. **Given** a Pants repo with BOTH `shell_source` targets (runtime
   scripts) AND `shunit2_test` targets (test scripts) in the same
   BUILD file, **When** operator runs `waybill sbom scan`, **Then**
   only the `shunit2_test`-owned components tag as development; the
   `shell_source`-owned components tag as runtime (or leave
   lifecycle-scope absent, matching Runtime default).

---

### Edge Cases

- **BUILD file references a missing script**: WARN + skip that
  target's file; continue scanning other targets. Scan does not
  abort (FR-009 fail-open).
- **BUILD file uses `sources=["*.sh"]` glob that matches zero
  files**: no components emitted for that target; INFO diagnostic
  naming the target + glob. Not a WARN — empty globs are legal.
- **BUILD file uses `sources=` with a subdirectory pattern
  (`"subdir/*.sh"`)**: waybill honors the pattern. `**` recursive
  globs also honored.
- **Two BUILD files at different depths both declare shell targets
  matching the SAME file** (rare but possible via explicit `source=`
  vs implicit glob overlap): emit one component per unique file,
  with a `waybill:pants-target` annotation containing BOTH target
  addresses (comma-separated), NOT two duplicate components.
- **BUILD file has syntactically-invalid Python-like content** (typo
  in a function call, etc.): waybill's regex-based extractor skips
  the malformed target with a WARN naming the BUILD file + line
  range. Other targets in the same file that DO parse are still
  emitted (per-target fail-open, not per-file).
- **Symlinked BUILD files**: canonical-path deduped so we don't emit
  the same target twice via two symlinks.
- **Files matching `.gitignore` inside a shell target's glob**:
  waybill does NOT filter — Pants sources declarations are the
  authoritative signal; `.gitignore` may exclude build artifacts
  that are still legitimate SBOM subjects.
- **`shell_command` targets** (Pants's arbitrary-command wrapper):
  waybill does NOT emit components for these in v1. They describe
  actions, not artifacts. Deferred; the follow-ups list in plan.md
  will capture this.
- **`pants.toml` under a nested subdirectory** (multi-repo layouts
  with per-project Pants installations): only the scan-root's
  `pants.toml` is consulted for `[shellcheck]` / `[shfmt]` version
  pins. Nested `pants.toml` files are skipped (Pants itself doesn't
  support nested configs).
- **Non-Pants repos** (no `BUILD` files present anywhere in the
  scan root): reader returns zero components and emits NO summary
  log line (byte-identity guarantee per FR-011 / SC-003).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The reader MUST walk `BUILD` files under the scan root
  using the existing `safe_walk` infrastructure (respects symlink
  cycle guards, `--exclude-path`, and depth limits).
- **FR-002**: The reader MUST extract `shell_source(name=..., source=...)`
  and `shell_sources(name=..., sources=[...])` target declarations
  from each discovered `BUILD` file.
- **FR-003**: The reader MUST extract
  `shunit2_test(name=..., source=...)` and
  `shunit2_tests(name=..., sources=[...])` target declarations from
  each discovered `BUILD` file.
- **FR-004**: For each target's resolved `.sh` file, the reader MUST
  emit ONE file-tier `pkg:generic/*` component with a SHA-256 hash
  computed over the file's on-disk bytes.
- **FR-005**: Every emitted component MUST carry a
  `waybill:pants-target` annotation containing the target's Pants
  address (e.g., `scripts:deploy`, `helpers:utils`). When multiple
  targets own the same file, the annotation MUST contain all owning
  addresses comma-separated in lexical order.
- **FR-006**: The reader MUST read `pants.toml` at the scan root
  (when present) and extract `version = "..."` keys from
  `[shellcheck]`, `[shfmt]`, and `[shunit2]` subsystem sections.
- **FR-007**: For each pinned tool discovered in FR-006, the reader
  MUST emit ONE design-tier `pkg:generic/<tool>@<version>` component
  with a `waybill:source-file=pants.toml` annotation.
- **FR-008**: Components owned by `shunit2_test` / `shunit2_tests`
  targets MUST tag `lifecycle_scope = Development`; components
  owned by `shell_source` / `shell_sources` targets MUST tag
  `lifecycle_scope = Runtime` (or leave absent — the Runtime
  default).
- **FR-009**: Per-file fail-open: any BUILD file with unrecoverable
  parse errors MUST log a WARN naming the file and be skipped;
  targets that parse successfully within an otherwise malformed
  BUILD file MUST still be emitted; the whole scan MUST NOT abort.
- **FR-010**: The reader MUST emit exactly one INFO log line at
  scan end summarizing counts with these structured fields:
  `build_files_discovered=N`, `build_files_parsed_ok=N`,
  `build_files_skipped_corrupt=N`, `shell_targets_found=N`,
  `script_components_emitted=N`, `tool_components_emitted=N`. When
  zero BUILD files are discovered AND no `pants.toml` is present,
  the reader MUST NOT emit any log line (byte-identity guarantee).
- **FR-011**: Repos with no Pants BUILD files AND no `pants.toml`
  MUST produce byte-identical SBOM output to a pre-feature-225
  scan of the same repo (SC-003 anchor).
- **FR-012**: The reader MUST NOT ingest scripts referenced only by
  `shell_command` targets in v1 — those are actions, not artifacts.
  Deferred per assumptions.

### Non-Functional Requirements

- **NFR-001**: Reader adds under 200 ms to scan runtime on a Pants
  monorepo with 100 BUILD files declaring 500 shell scripts total.
- **NFR-002**: Reader adds zero runtime cost on repos without any
  Pants BUILD files (early-return once the walker's first pass
  finds no `BUILD` files at the scan root).

### Key Entities

- **Shell target declaration**: one `shell_source` / `shell_sources`
  / `shunit2_test` / `shunit2_tests` invocation in a BUILD file.
  Carries a `name` (target address suffix) and either a `source`
  string or a `sources` list of glob patterns.
- **Resolved script**: the on-disk `.sh` file(s) that a target's
  `source=` / `sources=[...]` expression resolves to, relative to
  the BUILD file's own directory.
- **Pants target address**: canonical `<dir>:<name>` string
  identifying a target. Example: `scripts/BUILD` declaring
  `shell_source(name="deploy", ...)` has address `scripts:deploy`.
- **Pinned tool version**: `version = "..."` value inside a
  `[shellcheck]` / `[shfmt]` / `[shunit2]` `pants.toml` section.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Given a synthetic Pants repo with 5 `.sh` files across
  3 shell targets in 2 BUILD files, `waybill sbom scan` emits
  exactly 5 file-tier `pkg:generic/*` components, each with a
  SHA-256 hash matching the file bytes and a `waybill:pants-target`
  annotation. (US1 gate)
- **SC-002**: Given `pants.toml` with `[shellcheck] version = "v0.9.0"`
  and `[shfmt] version = "v3.7.0"`, `waybill sbom scan` emits
  exactly two `pkg:generic/*` design-tier components with the
  expected versions. (US2 gate)
- **SC-003**: Scanning a repo with zero Pants BUILD files and no
  `pants.toml` produces byte-identical SBOM output to a scan of the
  same repo built from `main` before this feature landed.
- **SC-004**: Components owned by `shunit2_test` /
  `shunit2_tests` targets tag `waybill:lifecycle-scope=development`;
  components owned by `shell_source` / `shell_sources` targets do
  NOT tag as development. (US3 gate)
- **SC-005**: A BUILD file containing 3 valid targets and 1
  syntactically-broken target emits components for the 3 valid
  targets, logs a WARN naming the broken target's line range, and
  the scan does not abort. (FR-009 fail-open gate)
- **SC-006**: A file owned by BOTH a `shell_source` target AND a
  `shunit2_tests` target's glob emits exactly ONE component whose
  `waybill:pants-target` annotation lists both addresses
  comma-separated in lexical order (edge-case dedup + provenance
  fidelity).

## Assumptions

- **BUILD file parsing is regex-based, not Python-interpreted**:
  Pants BUILD files use a Python-syntax DSL, but waybill remains
  pure Rust per Constitution Principle I (no embedded Python
  interpreter, no PyO3). The reader uses regex-based extraction
  scoped to the specific target function-call shapes listed in
  FR-002 / FR-003 — matches the approach in `cmake.rs`, `alpm.rs`,
  and other regex-driven readers.
- **Only the scan-root `pants.toml` is consulted**: nested
  `pants.toml` files under the scan root are ignored. Matches
  Pants's own behavior (Pants uses only the closest `pants.toml`
  above the CWD).
- **`shell_command` targets are deferred**: they describe actions
  (arbitrary shell invocations), not artifacts. Emitting them would
  require modeling "commands" as SBOM subjects, which is out of
  scope for a source-tier reader.
- **Pants default tool versions are NOT hardcoded**: waybill emits
  a tool component ONLY when the operator has explicitly pinned a
  version in `pants.toml`. Emitting Pants's built-in defaults would
  couple waybill to Pants's release cadence, which is a maintenance
  burden we intentionally avoid.
- **Reuses m223 `waybill:pants-target` catalog row** (verify shipping
  name at plan time — m223 shipped `waybill:pants-resolve` for the
  resolve-name; `pants-target` may or may not exist yet). If a new
  row is needed, one addition to `docs/reference/sbom-format-mapping.md`
  + matching entry in `parity/extractors/mod.rs::EXTRACTORS` per
  memory `feedback_sbom_format_mapping_extractor_gate`.
- **File-tier PURL shape**: `pkg:generic/<url-encoded-relative-path>@<sha256-prefix>`
  matches the m133 file-tier reader's convention. Planning-time
  verification: if m133 uses a different shape, spec adopts that
  shape instead.
- **BUILD-file discovery scope**: full recursive walk from scan
  root, honoring `--exclude-path`. `.gitignore` is NOT consulted
  (matches Pants's own behavior + waybill's other walkers).
- **Fixtures use synthetic script names**: `waybill-fixture-*.sh`,
  never real coordinates, per memory
  `feedback_fixture_synthetic_package_names`.

## Dependencies

- **Milestone 133** (file-tier component emission): reused for
  SHA-256 fingerprint helper + file-tier PURL construction. No API
  changes to m133 expected.
- **Milestone 054** (`safe_walk`): reused for BUILD-file discovery.
  Existing symlink-cycle guard + `--exclude-path` support.
- **Milestone 191** (PURL-level reconciler): no interaction expected
  — file-tier `pkg:generic/*` PURLs from this reader use a unique
  content-addressed shape that won't collide with any other reader.
- **Milestone 223** parity infrastructure: reuses (or extends) the
  `waybill:pants-target` / `waybill:pants-resolve` catalog rows.
  Reuses `waybill:source-file` (m080-shipped catalog row for
  metadata provenance).

## Out of Scope

- **`shell_command` target emission**: deferred.
- **shunit2 built-in bundle detection**: Pants ships an embedded
  shunit2; not enumerated as a component in v1 (only operator-pinned
  `[shunit2] version=...` is honored per FR-006/FR-007).
- **Cross-repo Pants workspaces** (`--workspace` mode): scope is
  per-scan-root only.
- **Custom user-defined shell target types** (via `pants.toml`
  plugin registration): only the built-in target types
  (`shell_source`, `shell_sources`, `shunit2_test`, `shunit2_tests`)
  are recognized. Plugin-registered target types are silently
  ignored.
