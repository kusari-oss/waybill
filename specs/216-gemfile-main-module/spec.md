# Feature Specification: Emit main-module for Gemfile-only Ruby applications

**Feature Branch**: `216-gemfile-main-module`
**Created**: 2026-07-22
**Status**: Draft
**Input**: User description: "fix bug #2 now" — see [waybill#629](https://github.com/kusari-oss/waybill/issues/629) for the discovered gap during m215 `--split` real-world validation on `~/Projects/iac`.

## Clarifications

### Session 2026-07-22

- Q: FR-002 — what PURL type + shape should Gemfile-only Ruby applications use? → A: `pkg:generic/<name>@<version>` + `waybill:package-shape = "application"` companion annotation. Rationale: the purl spec explicitly defines `pkg:gem/` as "RubyGems" with default repository URL `https://rubygems.org`. A bundler-managed Ruby application that isn't published to rubygems.org semantically doesn't fit `pkg:gem/`; using it would misrepresent the component per the spec's own type definition and could yield false-positive CVE cross-references from vuln scanners resolving the PURL back to a rubygems.org name-collision. `pkg:generic/` is the spec's explicit escape hatch (*"for plain, generic packages that do not fit anywhere else"*), and the companion annotation preserves the "this came from a Ruby Gemfile" ecosystem signal for consumers that want it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Split-mode scan surfaces Ruby applications as their own sub-SBOM (Priority: P1)

An operator running `waybill sbom scan --split --output-dir ./sboms/` against a polyglot monorepo containing bundler-managed Ruby applications (`Gemfile` + `Gemfile.lock`, no `.gemspec`) expects each Ruby application directory to produce its own sub-SBOM — the same way Cargo workspaces, npm workspaces, Go modules, pyproject dirs, and published gems do today.

**Why this priority**: The gap makes `--split` incomplete on any repo with Ruby applications. On the reproducer monorepo (`~/Projects/iac`), 3 of 3 Ruby application directories (`common-infra/`, `app-infra/`, `archives/gcp/`) silently produce no sub-SBOM, leaving operators to notice the missing files themselves. Split-mode's value proposition — "one SBOM per service" — is broken for Ruby shops.

**Independent Test**: Author a fixture directory containing `Gemfile` + `Gemfile.lock` with 2-3 declared deps and no `.gemspec`. Run `waybill sbom scan --path <fixture> --split --output-dir <tmp>`. Assert: a sub-SBOM named `<dirname>.<ecosystem>.<format-ext>.json` is emitted with the fixture's declared deps as `components[]`.

**Acceptance Scenarios**:

1. **Given** a directory containing `Gemfile` + `Gemfile.lock` and no `.gemspec`, **When** `waybill sbom scan --path <dir> --split --output-dir <out>` runs, **Then** exactly one sub-SBOM is emitted for that directory + the split manifest lists it as an entry with a valid `subproject_id` and `root_purl`.
2. **Given** a monorepo with N Ruby applications (each Gemfile-only) alongside M other-ecosystem workspaces, **When** `waybill sbom scan --path <repo> --split --output-dir <out>` runs, **Then** exactly N + M sub-SBOMs are emitted (previously N + M − k where k is the count of Ruby applications).
3. **Given** a directory containing BOTH a `.gemspec` AND a `Gemfile`, **When** the scan runs, **Then** exactly one main-module is emitted for that directory (the pre-existing gemspec path — Gemfile-inferred emission does not double-count when a gemspec already provides the identity).

---

### User Story 2 - Single-SBOM scan of a Ruby application gets a meaningful root component (Priority: P2)

An operator running `waybill sbom scan --path <ruby-app>` (no `--split`) on a single Ruby application currently sees `metadata.component.purl = pkg:generic/<dirname>@0.0.0` with the m127 root-selector heuristic annotation `synthetic-placeholder` — because no main-module component exists to select from. With this feature, the same scan yields a root component identifying the Ruby application by its inferred identity.

**Why this priority**: Same root cause as P1, different consumption path. Operators running non-split scans see the improved root selection automatically once the reader change lands.

**Independent Test**: Run `waybill sbom scan --path <gemfile-only-fixture>` (no `--split`). Assert `metadata.component.purl` does NOT match the `pkg:generic/<dirname>@0.0.0` synthetic-placeholder pattern and instead identifies the Ruby application.

**Acceptance Scenarios**:

1. **Given** a Gemfile-only directory, **When** a single-SBOM scan runs, **Then** the emitted SBOM's root component identifies the Ruby application (not the synthetic-placeholder fallback).

---

### Edge Cases

- **Nested Gemfiles under a top-level Gemfile**: sub-application inside another Ruby app. Should each level be its own main-module? (Assumption: yes — every Gemfile-carrying directory is treated as an application root regardless of whether a `Gemfile.lock` sibling exists per FR-006, matching how the cargo reader treats nested workspace members. The `Gemfile.lock` presence only affects the transitive-dep graph completeness, not the main-module emission decision.)
- **Gemfile without Gemfile.lock**: the app's transitive graph is undetermined. What happens? (Assumption: emit the main-module component anyway, but with the graph-completeness signal downgraded to reflect the missing lock — consistent with how the pip reader handles pyproject-without-lock.)
- **Application name derivation** when the Gemfile doesn't declare one: the current directory name is the only signal. Special-character handling matches the m215 slug rules for filename safety.
- **Gemfile shipped INSIDE a `.gemspec`-carrying directory**: unusual but possible. Precedence: `.gemspec` wins (matches the pre-existing gem reader path); no new main-module is emitted for the Gemfile.
- **Build-artifact directories** (e.g., a Ruby build tool emitting a scratch Gemfile under a `.output/` directory): already skipped by the walker's exclusion rules; nothing new to consider.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Waybill MUST emit a `waybill:component-role = "main-module"`-tagged component for every directory that contains a `Gemfile` AND does NOT contain a `.gemspec` file.
- **FR-002**: The synthetic main-module component MUST use PURL `pkg:generic/<name>@<version>` (per the purl-spec's own guidance: `pkg:gem/` is defined as "RubyGems" with default repository `https://rubygems.org`, so applying it to a bundler-managed application that isn't rubygems.org-published misrepresents the component). Every such component MUST carry a companion annotation `waybill:package-shape = "application"` to preserve the "this came from a Ruby Gemfile" ecosystem signal for consumers that want it.
- **FR-003**: The synthetic main-module's `name` field MUST derive from the containing directory basename when no better signal is present in the Gemfile. Sanitization follows the m215 slug rules (lowercase, unsafe-char stripping).
- **FR-004**: The synthetic main-module's `version` field MUST fall back to a stable placeholder (`0.0.0-unknown` — matching the Go module reader's `v0.0.0-unknown` convention) when neither the Gemfile nor a nearby signal (e.g. git tag) provides one.
- **FR-005**: When the Ruby application directory contains a companion `Gemfile.lock`, the reader MUST preserve the pre-existing transitive-dep parsing behavior; only the main-module component emission is new.
- **FR-006**: When a Ruby application directory contains a `Gemfile` but NO `Gemfile.lock`, the reader MUST still emit the main-module component and downgrade the graph-completeness signal appropriately (matching how the pip reader handles pyproject-without-lock).
- **FR-007**: When a `.gemspec` file is present in the same directory as a `Gemfile`, the gemspec-derived main-module (pre-existing path) MUST win; NO additional main-module is emitted for the Gemfile.
- **FR-008**: The new component MUST carry the `waybill:package-shape = "application"` annotation (per FR-002 resolution) so downstream consumers can distinguish Gemfile-derived main-modules from published-gem main-modules and from `pkg:generic/` components produced by other paths.
- **FR-009**: The change MUST NOT affect scans on directories with no `Gemfile` (backwards-compat guarantee for every non-Ruby scan).
- **FR-010**: The change MUST NOT affect scans on directories with only a `.gemspec` (backwards-compat guarantee for every published-gem scan — existing pre-feature byte-identity tests continue to pass).
- **FR-011**: `waybill sbom scan --split` MUST enumerate the new Ruby-application main-modules as split axes with no additional operator flag or configuration.
- **FR-012**: The emitted sub-SBOM filename for a Ruby application MUST follow the m215 `<slug>.<ecosystem>.<format-ext>.json` convention with `<ecosystem>` = `generic` (per FR-002's `pkg:generic/` choice). Filename example: `common-infra.generic.cdx.json`.

### Key Entities

- **Ruby application (Gemfile-only)**: A directory containing `Gemfile` (and typically `Gemfile.lock`) but no `.gemspec`. Represents a bundler-managed executable/service — the shape of every modern Rails app, every Ruby CI/deploy script, every non-published Ruby project.
- **Published gem**: A directory containing `.gemspec` (with or without `Gemfile.lock`). Represents a distributable Ruby library. Pre-existing reader path — unchanged by this feature.
- **Synthetic main-module component**: The `ResolvedComponent` this feature adds. Same shape as the pre-existing gemspec-derived main-module (same annotation set, same evidence structure), differing only in identity-derivation source (directory basename vs gemspec name) and possibly in PURL type and one distinguishing annotation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Running `waybill sbom scan --split` against the `~/Projects/iac` reproducer monorepo emits **37 sub-SBOMs** (was 34 pre-feature) — the 3 new sub-SBOMs correspond to `common-infra/`, `app-infra/`, `archives/gcp/`, and each has a filename of the form `<dirname>.generic.cdx.json` (per FR-012).
- **SC-002**: On a fixture directory containing `Gemfile` + `Gemfile.lock` with 2 declared deps and no `.gemspec`, a single-SBOM scan emits a root component whose `metadata.component.purl` matches `pkg:generic/<name>@<version>` and whose properties include `waybill:package-shape = "application"` (NOT the pre-feature `pkg:generic/<dirname>@0.0.0` synthetic-placeholder shape).
- **SC-003**: The `waybill:root-selection-heuristic` annotation on such a scan MUST report a heuristic OTHER than `synthetic-placeholder` (specifically `repo-root-main-module` — matching how the fix promotes the Ruby application to a first-class main-module candidate).
- **SC-004**: 100% of the pre-existing `cdx_regression`, `spdx_regression`, `spdx3_regression` byte-identity tests continue to pass (backwards-compat guarantee — no drift on non-Ruby or gemspec-carrying fixtures).
- **SC-005**: The `waybill:workspaces-detected` annotation on multi-workspace scans includes any newly-recognized Ruby application directories in its list.
- **SC-006**: In split-mode, the new Ruby-application sub-SBOMs pass the split-manifest v1 JSON schema validator (m215 SC-006 unchanged).

## Assumptions

- **Companion annotation** (per FR-002/FR-008 resolution): `waybill:package-shape = "application"` distinguishes Gemfile-derived main-modules from published-gem main-modules and from `pkg:generic/` components emitted by other paths. Value vocabulary `"application"` initially; may grow to include `"library"` / `"binary"` in future ecosystem-reader work. Precedent: milestone-004 US2 `waybill:binary-role` uses a similar single-key + closed-vocabulary pattern.
- **Version fallback**: When no version signal is present, use the string `"0.0.0-unknown"` — matches the Go reader's convention for unknown Go module versions and is already tolerated by every downstream consumer that parses waybill PURLs.
- **git-describe as version signal**: If the Ruby application directory is inside a git repository, waybill MAY use `git describe --tags --always` as a version-inference fallback (similar to the milestone-053 pattern for Go modules). This is a low-priority nicety; the `0.0.0-unknown` fallback is sufficient for MVP.
- **No new external tool dependencies**: The feature is entirely a static-parsing addition to the gem reader. No shelling out to `bundler` / `ruby` / `gem` binaries. Constitution Principle I preserved.
- **Global scope**: The change applies to BOTH single-SBOM and `--split` modes. There's no way to make the fix `--split`-only without introducing an asymmetry between the whole-repo scan's root selection and split-mode's subproject enumeration.
- **Rails-application heuristic**: Applications carrying `config.ru` or `Rakefile` alongside the Gemfile may in the future gain a `waybill:framework = "rails"` annotation. Out of scope for this feature; noted for a follow-up if operators request it.

## Dependencies

- Milestone 215 (`--split`) — merged. This feature builds on m215's split-axis enumeration signal (`waybill:component-role = "main-module"`), and its success criterion SC-001 is measured against m215's split-mode output.
- Milestone 127 (root-selector ladder) — merged. The new main-module component participates in the m127 ladder's `RepoRoot` fast-path, replacing the synthetic-placeholder fallback for Ruby applications.
- No new external crates; feature lives in the gem reader's existing parsing infrastructure.

## Out of Scope

- **Ruby version detection** from `.ruby-version` files — potentially useful metadata, but orthogonal to the main-module gap.
- **Framework classification** (Rails vs Sinatra vs plain-Ruby) — nice-to-have annotation, doesn't affect the main-module contract.
- **Bundler groups** (`:development`, `:test`, `:production`) — already handled per-component in the pre-existing gem reader; unchanged by this feature.
- **Gemfile-derived VCS remote inference** (populating `externalReferences[vcs]` from git remote origin) — separate feature; opens a broader design question about VCS-inference across all readers.
- **The reverse case** (published-gem shape without `.gemspec` but with a `Gemfile` that declares its own package name via `gemspec` DSL) — extremely rare, not observed on the reproducer repo, not blocking m215-split-mode completeness.
