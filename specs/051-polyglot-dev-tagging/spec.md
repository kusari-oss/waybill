# Feature Specification: Polyglot dev/test tagging — extend `mikebom:dev-dependency` to cargo and gem

**Feature Branch**: `051-polyglot-dev-tagging`
**Created**: 2026-05-01
**Status**: Draft

## Summary

Milestone 049 established the polyglot pattern for Go: emit every
lockfile entry as a component, tag test-only deps with
`mikebom:dev-dependency = true`, drop them when `--include-dev=off`.
Audit shows the pattern is already in place for several ecosystems:

| Ecosystem | Status (alpha.9) |
|---|---|
| **npm** | ✅ devDependencies tagged + dropped on `--include-dev=off` |
| **Poetry / Pipfile** | ✅ dev-dependencies tagged + dropped |
| **Maven** | ✅ `<scope>test</scope>` tagged + dropped (`maven.rs:1786-1823`) |
| **Go** | ✅ Milestone 049 (`_test.go` import-walk → tag + drop) |
| **Cargo** | ❌ `_include_dev: bool` parameter is **unused**; `is_dev: None` always |
| **Gem** | ❌ `_include_dev: bool` parameter is **unused**; `is_dev: None` always |

This milestone closes the gap for **cargo** and **gem**. After it
ships, every ecosystem mikebom supports will honor `--include-dev`
consistently, and SBOM consumers will see the same
`mikebom:dev-dependency = true` annotation everywhere a test/dev
dep is present.

## Clarifications

### Session 2026-05-01

- Q: Gem source of truth for group classification (lock-only vs.
  lock+Gemfile vs. lock+Gemfile+gemspec)? → A: Lock + Gemfile +
  `*.gemspec` (Option C — full coverage). Rationale: Ruby projects
  vary across formats (apps use `Gemfile` groups; libraries use
  `*.gemspec` `add_development_dependency`; older lockfiles don't
  carry group annotations). Reading all three sources gives the
  best classification accuracy. Production-wins-over-dev (FR-006)
  resolves any conflicts when sources disagree.

## User Scenarios & Testing

### User Story 1 - Cargo dev/build deps tagged (Priority: P1)

**As a developer scanning a Rust workspace**, when I run
`mikebom sbom scan --path ./my-rust-project`, I want crates
declared in `[dev-dependencies]` or `[build-dependencies]` of any
`Cargo.toml` to carry `mikebom:dev-dependency = true` in the SBOM
(or be dropped by default per the existing `--include-dev=off`
semantics) — same as how npm devDependencies and Maven
test-scope already work.

**Why this priority**: Cargo is the most-impactful gap. Every
non-trivial Rust project has dev-dependencies (criterion,
proptest, mockall, tempfile-as-test-helper, etc.) plus build-
dependencies (anything in `build.rs`'s build chain). Today they
all show up as runtime deps in the SBOM. Audit-grounded:
mikebom's own workspace lockfile has 12+ dev-dependencies that
currently emit unmarked.

**Independent Test**: Run `mikebom sbom scan --path
<rust-project-with-dev-deps> --output sbom.cdx.json` (default
mode). Assert: zero components carry the dev-dep crate names.
Run again with `--include-dev`. Assert: those names appear AND
each carries `mikebom:dev-dependency = true`.

**Acceptance Scenarios**:

1. **Given** a Rust project whose root `Cargo.toml` has
   `[dev-dependencies] criterion = "0.5"` and a `Cargo.lock`
   resolving criterion + its transitive closure,
   **When** I run `mikebom sbom scan --path .` (default),
   **Then** criterion and its dev-only transitives are absent
   from the SBOM.

2. **Given** the same project,
   **When** I run `mikebom sbom scan --path . --include-dev`,
   **Then** criterion and dev-only transitives appear AND each
   carries `mikebom:dev-dependency = true`.

3. **Given** a Rust workspace with a crate that's both a normal
   dep AND a dev-dep transitively,
   **When** I run any scan,
   **Then** the crate is treated as a normal dep (production wins
   over dev — same precedence rule as Go US2).

4. **Given** a `[build-dependencies]`-only crate (e.g., a
   build-script-only `cc` or `bindgen` use),
   **When** I run any scan,
   **Then** the crate is tagged `mikebom:dev-dependency = true`
   (build-deps don't ship in the runtime artifact, so they belong
   in the dev category for filtering purposes; same as how
   `[dev-dependencies]` aren't shipped).

---

### User Story 2 - Gem development/test group tagged (Priority: P1)

**As a developer scanning a Ruby project**, when I run
`mikebom sbom scan --path ./my-ruby-app`, I want gems declared
under `group :development` or `group :test` (or any non-default
group like `:doc`, `:lint`) in the `Gemfile`/`Gemfile.lock` to
carry `mikebom:dev-dependency = true` (or be dropped by default).

**Why this priority**: Same impact shape as cargo. Every Rails/
Sinatra/Hanami app has a substantial `:development` and `:test`
group (rspec, factory_bot, pry, byebug, capybara, etc.).
Currently these emit as runtime deps.

**Independent Test**: Run `mikebom sbom scan --path
<ruby-project>` (default). Assert: rspec / factory_bot / etc.
are absent. Run with `--include-dev`. Assert: they appear with
the dev-dep annotation.

**Acceptance Scenarios**:

1. **Given** a Ruby project with `Gemfile.lock` whose
   `DEPENDENCIES` block includes `rspec` after `group :test do`
   in the source `Gemfile`,
   **When** I run `mikebom sbom scan --path .` (default),
   **Then** rspec and its test-group-only transitives are absent.

2. **Given** the same project,
   **When** I run `mikebom sbom scan --path . --include-dev`,
   **Then** rspec emits with `mikebom:dev-dependency = true`.

3. **Given** a gem in BOTH a default group AND `:test`,
   **When** I run any scan,
   **Then** the gem is treated as a runtime dep (production wins).

---

### User Story 3 - Maven existing behavior is regression-tested (Priority: P2)

**As an existing mikebom user scanning Java/Maven projects**,
when this milestone ships, I want the existing
`<scope>test</scope>` handling at `maven.rs:1786-1823` to be
unchanged: test-scope deps are dropped on default, tagged on
`--include-dev`. No regression.

**Why this priority**: Maven already works correctly per
audit; adding regression coverage just guards the contract.

**Independent Test**: Existing maven integration tests pass
unchanged. Add one new explicit assertion that
`<scope>test</scope>` deps carry `mikebom:dev-dependency = true`
when `--include-dev` is set.

**Acceptance Scenarios**:

1. **Given** a Maven project with a test-scope dep (junit),
   **When** I run `mikebom sbom scan --path . --include-dev`,
   **Then** junit emits with `mikebom:dev-dependency = true`
   AND its component count is unchanged from alpha.9 baseline.

---

### Edge Cases

- **Cargo workspace with mixed dev/non-dev usage across
  member crates**: a crate that's a dev-dep of crate A but a
  normal dep of crate B should be PRODUCTION (production wins,
  same as Go US2). This requires a workspace-wide BFS, not
  per-Cargo.toml local analysis.
- **Cargo `target."cfg(...)".dev-dependencies`** (target-
  conditional dev deps): tag as dev (same semantic as
  unconditional `[dev-dependencies]`).
- **Gem `gemspec` development_dependencies vs Gemfile groups
  vs lock annotations**: per Q1 / FR-004, all three sources are
  read and unioned (with production winning conflicts). Library
  gems shipping only a `*.gemspec` get tagged via
  `add_development_dependency` calls; apps using `Gemfile` get
  tagged via `group :foo do ... end` blocks; modern Bundler
  apps additionally benefit from lock-side group annotations.
- **Gem with no group annotation anywhere**: default group =
  production. Untagged. Same default semantic as before.
- **Maven `<scope>provided</scope>`**: out of scope — these
  are present at compile time but not bundled at runtime.
  They're conceptually similar to Go's `mikebom:not-linked`
  (milestone 050). Tracked as a separate future milestone.
- **Maven `<optional>true</optional>`**: out of scope — already
  handled correctly per current maven.rs; no behavior change.
- **rpm `Recommends:` / `Suggests:`**: out of scope — soft
  runtime deps, not dev-deps in this sense.

## Requirements

### Functional Requirements

- **FR-001 (Cargo)**: When the cargo reader walks a Rust
  project, every crate reachable ONLY through
  `[dev-dependencies]` or `[build-dependencies]` edges (across
  all `Cargo.toml` files in the workspace) MUST be tagged
  `is_dev = Some(true)` in its `PackageDbEntry`.
- **FR-002 (Cargo)**: When `include_dev = false`, tagged
  cargo entries MUST be dropped (mirrors maven.rs's existing
  pattern at lines 1786-1823).
- **FR-003 (Cargo)**: A crate reachable from BOTH a normal-dep
  edge AND a dev/build-dep edge MUST be tagged production
  (production wins). This requires the same set-union semantics
  as Go US2 / npm prod-vs-dev.
- **FR-004 (Gem)**: When the gem reader walks a Ruby project,
  every gem the union of three sources classifies as
  development/test MUST be tagged `is_dev = Some(true)`. The
  three sources, in priority order:
    1. **`Gemfile.lock`** `DEPENDENCIES` block — newer Bundler
       versions emit group annotations directly; honor them when
       present.
    2. **`Gemfile`** — parse `group :name do ... end` blocks
       and inline `gem "...", group: :name` syntax. Authoritative
       when the lockfile is too old to carry group metadata.
    3. **`*.gemspec`** at the project root — parse
       `s.add_development_dependency "..."` invocations.
       Authoritative for library-style gems that don't ship a
       `Gemfile`.
  Transitive gems reachable ONLY from grouped roots MUST also be
  tagged. Group names other than the default (`:default`) all
  count as dev for filtering purposes (`:development`, `:test`,
  `:doc`, `:lint`, custom groups, etc.).
- **FR-005 (Gem)**: When `include_dev = false`, tagged gem
  entries MUST be dropped.
- **FR-006 (Gem)**: A gem classified as production by ANY of
  the three sources MUST be tagged production, even if another
  source classifies it as dev (production wins, mirroring the
  Go US2 / npm rule). Sources are unioned for prod evidence,
  intersected for dev evidence.
- **FR-007 (Maven)**: Existing test-scope behavior at
  `maven.rs:1786-1823` MUST be unchanged. The new milestone
  test in `tests/scan_maven.rs` MUST explicitly assert
  `mikebom:dev-dependency = true` appears when `--include-dev`
  is set.
- **FR-008**: Existing 27 byte-identity goldens MUST stay
  byte-identical UNLESS the underlying fixture exercises
  dev/test deps. Audit during implementation: regenerate ONLY
  fixtures with dev/test signals, leave others untouched.
- **FR-009**: All 11 holistic_parity tests MUST continue to
  pass — the existing C6 (`mikebom:dev-dependency`) parity
  wiring picks up the new cargo/gem tagging automatically.
- **FR-010**: No new flag, no new annotation, no new catalog
  row. This milestone is pure additive population on the
  existing C6 infrastructure.

### Key Entities

- **`PackageDbEntry.is_dev: Option<bool>`** (existing): set to
  `Some(true)` on tagged cargo / gem entries.
- **Cargo workspace dep graph**: built from all `Cargo.toml`
  files (root + workspace members) + `Cargo.lock` resolved
  package list. BFS produces the prod-reachable set; cargo
  entries NOT in that set are dev/build.
- **Gem grouping signals** (multi-source per FR-004):
  - **`Gemfile.lock` `DEPENDENCIES`**: newer Bundler emits
    group annotations inline.
  - **`Gemfile`**: `group :name do ... end` blocks and inline
    `gem "...", group: :name` syntax.
  - **`*.gemspec`** at project root:
    `s.add_development_dependency` invocations.
  Direct-dep grouping flows transitively through the lockfile's
  `specs:` indent-6 edges (existing parser).

## Success Criteria

### Measurable Outcomes

- **SC-001 (Cargo)**: Scanning the mikebom-self workspace
  (`/Users/mlieberman/Projects/mikebom`) in default mode emits
  ≥ 5 fewer components than alpha.9 (the existing
  `[dev-dependencies]` get dropped).
- **SC-002 (Cargo)**: Scanning the same workspace with
  `--include-dev` emits the same component count as alpha.9
  AND ≥ 5 components carry `mikebom:dev-dependency = true`.
- **SC-003 (Gem)**: Scanning a typical Ruby project with
  `group :development` and `group :test` emits ≥ 3 fewer
  components in default mode vs alpha.9.
- **SC-004 (Gem)**: Same project with `--include-dev` emits
  the alpha.9 count with the new components tagged.
- **SC-005 (Maven)**: All existing maven integration tests
  pass unchanged.
- **SC-006**: 27 byte-identity goldens pass after regen (any
  cargo/gem fixture with dev-deps will see component count
  drop in default-mode goldens; non-dev-affected ecosystems
  unchanged).
- **SC-007**: 11/11 holistic_parity passes — the C6 row's
  existing parity wiring picks up cargo/gem `is_dev`
  population automatically.
- **SC-008**: At least one new integration test per ecosystem
  (cargo, gem, maven regression) covers the dev-tagging path.
- **SC-009**: `pre-pr.sh` clean.
- **SC-010**: 3 CI lanes green.

## Assumptions

- The milestone-049 `--include-dev` pattern is the right shape
  for cargo and gem (same as it was for Go and the others).
  Validated by the user's verbatim ask to extend the pattern
  ("cargo, maven, gem, etc.") and the existing parallel wiring
  for npm/Poetry/Pipfile/Maven.
- "Production wins over dev" is the correct precedence when a
  module is reachable from both — same rule as milestone 049
  US2 (`scan_go_source_production_and_test_import_dominates`).
  An SBOM consumer trying to filter dev-deps wants conservative
  retention; a transitively-prod dep should never be silently
  dropped just because it's also reachable through a dev edge.
- `[build-dependencies]` are tagged dev (not in shipped
  binary). Per the cargo book, build-deps are only present at
  compile time, not in the resulting binary — same semantic as
  dev-deps for SBOM filtering.
- Cargo workspace dep-graph traversal stays in-process
  (parsing `Cargo.toml` files directly + `Cargo.lock`'s
  resolved list); no `cargo metadata` shell-out. Mikebom
  is a Rust binary that doesn't depend on a host cargo install.
- Maven `<scope>provided</scope>` and `<scope>runtime</scope>`
  enhancements are out of scope — separate future milestone
  (potentially via a new annotation parallel to milestone 050's
  `mikebom:not-linked`).
- npm / Poetry / Pipfile dev-dep handling is already correct
  (alpha.9 audit: `--include-dev` flag is honored, dev-deps
  carry C6 annotation when emitted). No work needed here.
- rpm `Recommends:` / `Suggests:` are NOT dev-deps in the
  same sense — they're soft runtime deps. Out of scope.
