---
description: "Plan — milestone 051 polyglot dev/test tagging (cargo + gem + maven regression)"
---

# Plan: Polyglot dev/test tagging — `mikebom:dev-dependency` for cargo and gem

**Branch**: `051-polyglot-dev-tagging` | **Spec**: spec.md ✅
**Output**: 4-file tighter template (no research.md / data-model.md /
contracts/ / quickstart.md — pattern from
021/022/023/042/046/047/048/049/050).

## Constitution Check

Reviewed against `.specify/memory/constitution.md`. No violations:

- **I. Pure Rust, Zero C**: parsing TOML (`Cargo.toml`) uses
  existing `toml` crate (already in deps via `cargo.rs`). Parsing
  `Gemfile` and `*.gemspec` via hand-rolled byte/regex scanners
  (mikebom's house style for non-TOML/non-JSON formats — see
  `gem.rs`'s existing parser). No new C deps.
- **III. Fail Closed**: existing cargo.rs already fails closed on
  v1/v2 lockfiles. New Cargo.toml parser should warn-and-skip on
  unparseable files (preserving the "broken project shouldn't
  abort the whole scan" precedent set by gem.rs / pip.rs).
  Confirmed safe — see Phase 0 R3.
- **IV. Type-Driven Correctness**: reuse existing
  `is_dev: Option<bool>` field on `PackageDbEntry`.
  Three-state (`Some(true)` / `Some(false)` / `None`) preserved.
- **VIII. Completeness / IX. Accuracy**: this milestone directly
  improves both — no deps silently miscategorized as runtime when
  they're dev-only.
- **No new annotation, no new flag, no new catalog row**.

## Phase 0: Recon (resolved inline; no `research.md`)

### R1. Cargo: Cargo.lock has resolved dep edges; no Cargo.toml parsing today

**Finding**: `mikebom-cli/src/scan_fs/package_db/cargo.rs:323`
(`pub fn read(rootfs: &Path, _include_dev: bool)`) parses
`Cargo.lock` only. The `_include_dev` parameter is unused
(underscore-prefixed). Cargo.lock contains the full resolved
graph via `[[package]] dependencies = [...]` arrays — but
NOT the dev/build/runtime classification (the lockfile
flattens all three into a single `dependencies` list per
package).

**Decision**: add Cargo.toml parsing alongside Cargo.lock
parsing. For each scanned `Cargo.lock`, walk the same
directory + workspace-member directories for `Cargo.toml`
files; extract `[dev-dependencies]`, `[build-dependencies]`,
`[dependencies]` sections; build the dev-roots set; BFS
through lockfile dep edges to compute prod-reachable closure;
tag entries OUTSIDE the prod closure with `is_dev=Some(true)`.

**Workspace traversal**: a workspace's root `Cargo.toml` may
declare `[workspace] members = ["crate-a", "crate-b/*"]` with
glob patterns. Implement glob resolution via existing path
walk (no `glob` crate needed; the patterns are simple `*` /
`**` cases). For each member crate, parse its own
`Cargo.toml`'s sections.

**Target-conditional sections** (`target."cfg(unix)".dev-dependencies`):
spec FR-001 says treat as dev. Implementation: walk all
top-level `target.<cfg>.{dev,build}-dependencies` keys via
TOML `Table` iteration; classify same as unconditional
sections.

### R2. Gem: three-source classification (lock + Gemfile + gemspec)

Per Q1 / FR-004 (clarified 2026-05-01):

**Gemfile.lock** (existing parse): newer Bundler emits group
annotations under `DEPENDENCIES` like:

```
DEPENDENCIES
  rspec (~> 3.0)
    group: test
```

Older locks omit the group line. Existing parser at
`gem.rs:114-145` reads the gem name; needs extension to read
the optional `group:` continuation line.

**Gemfile** (NEW parser): handles the canonical app shape:

```ruby
group :development, :test do
  gem "rspec"
  gem "factory_bot"
end
gem "pry", group: :development
gem "rack"  # default group → production
```

Hand-rolled scanner — patterns are line-oriented and small.
No need for a Ruby parser; we only extract `gem "name"` calls
and their group context. Bundler doesn't enforce strict syntax
beyond the keyword form, so we treat the Gemfile parser as
best-effort: warn-and-skip on unparseable lines (mirrors
gem.rs's existing tolerance for malformed `Gemfile.lock`
specs).

**`*.gemspec`** (NEW parser): handles the canonical library
shape:

```ruby
Gem::Specification.new do |s|
  s.add_dependency "activesupport"
  s.add_development_dependency "rspec"
end
```

Same line-oriented hand-rolled scanner. Match
`s.add_dependency "..."` (or `add_runtime_dependency`) for
prod, `s.add_development_dependency "..."` for dev.

**Union semantics** per FR-006: gem is prod IFF ANY source
classifies it as prod (default group, `add_dependency`, or
prod transitively via lockfile spec edges). Otherwise dev.

### R3. Cargo.toml parse failure handling

**Decision**: warn-and-skip on unparseable Cargo.toml files
(don't fail the whole scan). Mirrors the existing pattern at
`cargo.rs:252-258` for unparseable `Cargo.lock`. Constitution
III's "fail closed" applies to invariant violations
(unsupported lockfile versions); a malformed `Cargo.toml`
in some workspace member is recoverable degradation — emit
the entries WITHOUT dev classification rather than abort.

### R4. Maven: existing test-scope handling at `maven.rs:1786-1823`

**Existing primitives** verified:

```rust
// mikebom-cli/src/scan_fs/package_db/maven.rs:1786
if !include_dev && matches!(dep.scope.as_deref(), Some("test")) {
    continue;
}
// mikebom-cli/src/scan_fs/package_db/maven.rs:1823
is_dev: matches!(dep.scope.as_deref(), Some("test")).then_some(true),
```

**Decision**: zero code change. US3 (P2) requires only a new
explicit integration test in `tests/scan_maven.rs` asserting
`mikebom:dev-dependency = true` appears on test-scope deps
when `--include-dev` is set. Regression guard against future
refactors.

### R5. Existing infrastructure (no change)

- **C6 catalog row** (`docs/reference/sbom-format-mapping.md:51`):
  documents `mikebom:dev-dependency` parity contract. Already
  applies to cargo and gem outputs (no per-ecosystem clauses).
- **CDX/SPDX 2.3/SPDX 3 serializers**: gate emission on
  `is_dev == Some(true)` (`cyclonedx/builder.rs:317`,
  `spdx/annotations.rs:148`, `spdx/v3_annotations.rs:164`).
- **Parity extractors**: existing `cdx_dev_deps`,
  `spdx23_dev_deps`, `spdx3_dev_deps` already wire C6 across
  formats.
- **`--include-dev` plumbing** at `cli/scan_cmd.rs:584`
  threads through to `read_all` → individual readers.
- **`apply_go_production_set_filter` precedent** (milestone
  049): drop-on-`!include_dev` pattern lifted directly into
  cargo/gem.

## Phase 1: Implementation strategy

Single PR, three commits (one per ecosystem) for clean review:

### Commit 1 — `feat(051/us1): cargo dev/build dep tagging`

**Touched files**:

- **`mikebom-cli/src/scan_fs/package_db/cargo.rs`** (~120 LOC)
  - Add a `CargoTomlSections` struct holding three
    `HashSet<String>`: `prod_deps`, `dev_deps`, `build_deps`.
  - Add `parse_cargo_toml(path: &Path) -> Option<CargoTomlSections>`
    — TOML parse; iterate `[dependencies]`, `[dev-dependencies]`,
    `[build-dependencies]`, `target.*.{dev,build}-dependencies`,
    extract crate names. Warn-and-skip on parse error per R3.
  - Add `discover_workspace_manifests(rootfs: &Path) ->
    Vec<PathBuf>` — find every `Cargo.toml` in workspace
    (root + members); resolve glob patterns inline
    (no new crate dep).
  - Add `compute_cargo_prod_set(lock: &CargoLock,
    direct_prod: HashSet<String>) -> HashSet<(String, String)>`
    — BFS through `[[package]] dependencies = [...]` edges
    starting from direct prod crate names, collecting
    `(name, version)` tuples reachable through prod chains.
    Returns the prod-reachable closure.
  - Modify `parse_lockfile` to also accept the prod-set
    parameter and tag `is_dev = Some(true)` on entries OUTSIDE
    the prod set.
  - Modify `read` to: (1) drop `_` from `include_dev`
    parameter, (2) discover workspace manifests, (3) compute
    prod set per workspace, (4) pass to lockfile parsing,
    (5) drop tagged entries when `!include_dev`.

- **`mikebom-cli/tests/scan_cargo.rs`** (~80 LOC)
  - New integration test
    `scan_cargo_dev_dependency_is_tagged_and_droppable` —
    synthetic workspace with root `Cargo.toml` + dev-dep
    + `Cargo.lock`. Default scan: dev-dep absent. With
    `--include-dev`: dev-dep present + tagged.
  - New integration test
    `scan_cargo_build_dependency_is_treated_as_dev` —
    `[build-dependencies]`-only crate gets tagged.
  - New integration test
    `scan_cargo_production_wins_over_dev` — crate reachable
    via both prod and dev edges retained as prod.

**Verification** (Commit 1):

- New unit tests in `cargo.rs::tests` for `parse_cargo_toml`
  and `compute_cargo_prod_set`.
- 3 new integration tests pass.
- Existing 8 cargo tests pass.
- `cdx/spdx/spdx3` regression: simple-cargo fixture's golden
  may shift if it has a dev-dep (audit during impl); regen if
  needed.
- Real-world smoke on the mikebom workspace:
  `mikebom sbom scan --path /Users/mlieberman/Projects/mikebom`
  default emits ≥ 5 fewer cargo components vs alpha.9 (per
  SC-001).

### Commit 2 — `feat(051/us2): gem development/test group tagging`

**Touched files**:

- **`mikebom-cli/src/scan_fs/package_db/gem.rs`** (~150 LOC)
  - Extend `parse_gemfile_lock` to read group annotations
    when present in the lock's `DEPENDENCIES` block (lines
    133+). Add a `groups: HashMap<String, Vec<String>>`
    field on `GemfileLockDocument` (gem name → groups).
  - Add `parse_gemfile(path: &Path) -> HashMap<String, Vec<String>>`
    — line-oriented scanner for `group :name do ... end`
    blocks and inline `gem "...", group: :name`.
    Best-effort; warn-and-skip on syntax outside the
    canonical idioms.
  - Add `parse_gemspec(path: &Path) -> HashMap<String, Vec<String>>`
    — same scanner shape; matches
    `s.add_dependency "..."` / `s.add_runtime_dependency
    "..."` (prod) and `s.add_development_dependency "..."`
    (dev).
  - Add `compute_gem_prod_set(direct_prod: HashSet<String>,
    lock: &GemfileLockDocument) -> HashSet<String>` — BFS
    through `lock.specs` indent-6 transitive edges from
    prod roots. Returns prod-reachable gem names.
  - Modify `read` to: (1) drop `_` from `include_dev`,
    (2) parse Gemfile.lock as today, (3) parse co-located
    Gemfile + `*.gemspec`, (4) compute union prod set per
    FR-006, (5) tag non-prod entries, (6) drop tagged when
    `!include_dev`.

- **`mikebom-cli/tests/scan_gem.rs`** (~80 LOC)
  - New integration tests covering: lockfile group
    annotations, Gemfile groups, gemspec dev deps, union
    semantics with conflicting sources.

**Verification** (Commit 2):

- 4+ new integration tests pass.
- Existing 3 gem tests pass.
- Real-world smoke: synthetic Ruby fixture with mixed
  Gemfile groups + gemspec dev deps emits ≥ 3 fewer
  components in default mode.

### Commit 3 — `feat(051/us3): maven dev-dep regression test + chore scaffolding`

**Touched files**:

- **`mikebom-cli/tests/scan_maven.rs`** (~30 LOC)
  - New integration test
    `scan_maven_test_scope_is_tagged_with_include_dev` —
    synthetic Maven project with `<scope>test</scope>` dep.
    With `--include-dev`: junit emits with
    `mikebom:dev-dependency = true`. Without: junit absent.
  - Regression guard against future refactors that might
    inadvertently break the existing maven.rs:1823 wiring.

- **`CHANGELOG.md`** (~10 LOC)
  - `[Unreleased]` → `### Changed`: name the cargo + gem dev-
    tagging, no behavior change for npm/Poetry/Pipfile/Maven,
    no new flag/annotation/catalog-row.

- **`specs/051-polyglot-dev-tagging/`** scaffolding
  (spec/plan/tasks/checklists already authored; bundle
  into this commit per the established 4-file pattern).

**Verification** (Commit 3):

- `pre-pr.sh` clean from a fresh shell.
- 11/11 holistic_parity ok (existing C6 wiring picks up new
  is_dev population automatically).
- 27/27 byte-identity goldens pass after any audit-driven
  regen.

## Touched files

| File | Commit | LOC |
|---|---|---|
| `mikebom-cli/src/scan_fs/package_db/cargo.rs` | 1 | +120 |
| `mikebom-cli/tests/scan_cargo.rs` | 1 | +80 |
| `mikebom-cli/src/scan_fs/package_db/gem.rs` | 2 | +150 |
| `mikebom-cli/tests/scan_gem.rs` | 2 | +80 |
| `mikebom-cli/tests/scan_maven.rs` | 3 | +30 |
| `CHANGELOG.md` | 3 | +10 |
| `specs/051-polyglot-dev-tagging/` | 3 | scaffolding |

Total: ~470 LOC of Rust + scaffolding. Single PR, three
commits ordered for review clarity.

## Risks

- **R1: Cargo workspace member traversal complexity.** Some
  workspaces declare members via glob patterns
  (`members = ["crates/*"]`). Implementation must resolve
  these without a new crate dep (the existing path-walk
  primitives suffice — see `cargo.rs::find_cargo_lockfiles`).
  If glob complexity escalates, fall back to "scan every
  `Cargo.toml` reachable under rootfs" — same effective
  semantics for typical layouts.

- **R2: Gemfile syntax edge cases.** Bundler accepts a wide
  range of Ruby DSL forms (interpolation, conditional
  loading, `eval_gemfile`). Spec-grade compliance would need
  a Ruby parser. Decision per R2: best-effort line-scanner
  matching the canonical idioms; warn-and-skip the rest.
  If a downstream consumer reports a missed group, the lock
  + gemspec sources usually catch it via the union semantics.

- **R3: Golden churn on simple-cargo and simple-gem
  fixtures.** Both fixtures may have dev/test deps — audit
  during impl. If goldens shift, regen is mechanical (same
  `MIKEBOM_UPDATE_*_GOLDENS=1` flow as milestone 049).

- **R4: `--include-dev` flag interaction with the
  milestone-049 Go path and milestone-050 not-linked
  annotation**. The flag is shared across all ecosystems.
  Verify during impl that toggling it doesn't disturb the
  Go test-only / cache-zip / not-linked flows. The
  `apply_go_production_set_filter` and
  `apply_go_linked_filter` chains are independent of the
  cargo/gem readers — confirmed via code inspection.

## Out of scope

- **Maven `<scope>provided</scope>` / `<scope>runtime</scope>`
  tagging**: separate future milestone (potentially via a
  new annotation parallel to `mikebom:not-linked`).
- **Cargo `[features]` flag-gated deps**: a crate may be in
  `[dependencies]` but conditional on a feature. Today's
  Cargo.lock resolves features at lock-time and emits the
  unioned closure — that's what mikebom reads. Per-feature
  conditional tagging is out of scope.
- **rpm `Recommends:` / `Suggests:` soft-dep tagging**:
  semantically different from dev/test; no work.
- **npm / Poetry / Pipfile**: already correctly tagged per
  alpha.9 audit; no work.
