# Research — milestone 143 Haskell reader (Phase 0)

Resolves all Technical Context unknowns before Phase 1 design. Decisions either inherit from prior milestones (milestone 070 Maven for `pkg:<registry>/` PURL shape posture, milestone 141 Erlang for Q1 stdlib-allowlist pattern, milestone 142 Scala for multi-project union discovery + Q3 content-shape validation pattern), or are Haskell-specific (the Cabal-DSL stanza syntax, `cabal.project.freeze` constraint format, `stack.yaml.lock` schema, the Stackage snapshot-resolver convention).

## R1 — PURL spec audit for Hackage

**Decision**: All Haskell components emit PURL `pkg:hackage/<lc-name>@<version>`. The `hackage` type is purl-spec-blessed ([hackage-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/hackage-definition.md)). No namespace component (Hackage doesn't have a namespace concept), no qualifiers required for default cases.

**Hackage name casing**: Hackage's website permits mixed-case names (`MissingH`, `case-insensitive`) but treats package names case-insensitively for resolution. The purl-spec hackage-definition specifies lowercase canonical form. mikebom emits lowercased names; `MissingH` and `missingh` resolve to the same PURL.

**Stackage snapshot placeholder**: Per FR-005, Stackage snapshots emit `pkg:generic/<resolver-name>@<sha-or-unspecified>` (NOT `pkg:hackage/`). The `pkg:generic/` choice is deliberate — the placeholder represents a curated bundle, not a single Hackage package; using `pkg:hackage/` would falsely imply individual-package identity. The `mikebom:stackage-resolver = "<resolver-name>"` annotation preserves the operator-facing identifier (`lts-22.0` / `nightly-2024-01-15` / `ghc-9.6.4`) for downstream tooling that wants to correlate the bundle without parsing the PURL.

**Rationale**: The purl-spec audit is the same shape as milestone 142's Scala/Maven Central audit — single-registry ecosystem with a blessed PURL type, no surprises. Hackage is much simpler than Maven Central (no namespace, no Scala-version suffix shenanigans, no cross-built variants).

**Alternatives considered**:

- Use `pkg:hackage/` for Stackage snapshot placeholders — REJECTED. Falsely implies individual-package identity for a curated bundle.
- Add a `pkg:stackage/` PURL type — REJECTED. Not purl-spec-blessed; would require submission of a new type definition; not worth it for one placeholder per project.
- Skip Stackage placeholders entirely — REJECTED per FR-005 + spec User Story 2 acceptance scenarios. Operators need the snapshot-resolver identity in the SBOM for VEX correlation against known-vulnerable-snapshot bundles.

## R2 — `cabal.project.freeze` line-format parsing

**Decision**: The freeze format is a single top-level `constraints:` keyword followed by a comma-separated list of `<pkg> ==<version>` exact-pin or `<pkg> +<flag>` / `<pkg> -<flag>` flag-toggle entries. Multi-line continuations: the format permits the constraints list to span multiple lines (the comma is the entry separator; whitespace between entries is irrelevant). The reader's parse pipeline:

1. Read the file, locate the `constraints:` keyword.
2. Concatenate all subsequent lines into one logical line (preserving commas + dropping whitespace).
3. Split on commas to get individual constraint entries.
4. Per entry, regex-dispatch into one of three shapes:
   - `<name> ==<version>` → exact pin → emit `pkg:hackage/<name>@<version>` source-tier component (FR-002 + FR-004)
   - `<name> +<flag>` / `<name> -<flag>` → flag toggle → SKIP per Edge Case
   - `<name> >=<version> && <<version>` (or other range operators) → range constraint → emit design-tier with `mikebom:requirement-range` evidence (FR-002 trailing clause)

**Regex set** (all hoisted to `OnceLock` at module scope per the milestone-141 R7 + milestone-142 R8 lesson):

- `CONSTRAINTS_KEYWORD_RE` — `(?ms)^constraints:\s*(.+?)(?:^\w|\z)` (multiline mode, captures everything after `constraints:` until the next top-level keyword or EOF)
- `EXACT_PIN_RE` — `^([a-zA-Z][a-zA-Z0-9-]*)\s+==\s*(\d[\d\.]*(?:-[a-zA-Z0-9]+)?)$`
- `FLAG_TOGGLE_RE` — `^([a-zA-Z][a-zA-Z0-9-]*)\s+[+-]([a-zA-Z][a-zA-Z0-9-]*)$`
- `RANGE_CONSTRAINT_RE` — `^([a-zA-Z][a-zA-Z0-9-]*)\s+(.+)$` (catch-all; the range string is preserved verbatim as `mikebom:requirement-range`)

**Rationale**: The freeze format is fully regular — no nested structures, no escaping, no line-format dependencies on whitespace beyond the keyword-and-colon separator. Regex extraction is sufficient for the full grammar.

**Alternatives considered**:

- Full Cabal-format parser (use the `cabal-install` library directly) — REJECTED. Requires GHC + cabal at compile time + would link a Haskell runtime; constitution Principle I (Pure Rust Zero C, no Haskell either). Regex covers the entire freeze grammar.
- Line-by-line parsing without the multi-line continuation pass — REJECTED. Some real-world freeze files (e.g., from `cabal v2-freeze` on a 100+-dep project) emit a single `constraints:` keyword followed by 100+ comma-separated entries spread across continuation lines.

## R3 — `stack.yaml.lock` YAML parsing

**Decision**: Use the workspace `serde_yaml = "0.9"` crate (already a dep per milestones 137 dart + 139 cocoapods). The Stack lockfile schema is stable since Stack 2.1 (2019); the relevant fields:

```yaml
# Lock file, version 1
snapshots:
  - completed:
      sha256: "<64-hex>"
      size: <integer>
      url: "https://raw.githubusercontent.com/commercialhaskell/stackage-snapshots/master/lts/22/0.yaml"
    original:
      resolver: lts-22.0
packages:
  - completed:
      hackage: aeson-2.2.0.0@sha256:<hash>,<size>
      pantry-tree:
        sha256: "<64-hex>"
        size: <integer>
    original:
      hackage: aeson-2.2.0.0
```

Parse into typed structs via `serde_yaml::from_str::<StackYamlLock>(text)`. The reader extracts:

- `snapshots[].original.resolver` → snapshot-resolver identifier (drives `mikebom:stackage-resolver`)
- `snapshots[].completed.sha256` → snapshot SHA (drives the placeholder's PURL version)
- `packages[].original.hackage` → `<name>-<version>` string for explicit extra-deps → split on the LAST dash to recover name + version, emit `pkg:hackage/<lc-name>@<version>` source-tier component

**Content-shape validation gate** (mirrors milestone-142 Q3): require top-level `snapshots:` key as an array before treating the file as authoritative. Files matching the `stack.yaml.lock` name but lacking this structure warn-and-skip per FR-009.

**Rationale**: serde_yaml is the cross-milestone-established YAML parser; no new deps. The Stack lockfile schema is well-documented and stable. Optional `extra-deps` from git repos (e.g., `original: {git: ..., commit: ...}`) parse as a separate variant; v1 emits them as `pkg:generic/<name>@<commit>?vcs_url=git+<url>` per the milestone-140/141/142 git-source precedent — though we'll likely defer git-source support in `stack.yaml.lock` to v1.1 to keep US2 scope tight (note as Out-of-Scope unless we already had it in the spec — actually spec only mentions Hackage extra-deps; defer git-source variants explicitly).

**Decision update** (out-of-scope flag for spec): `stack.yaml.lock` git-source `extra-deps` (e.g., `original: {git: "https://github.com/foo/bar.git", commit: "<sha>"}`) are out of scope for v1; warn-and-skip. Operators using git-source extra-deps in Stack are rare (~5% of Stack projects); add to spec's Out-of-Scope section during implementation if not already present.

**Alternatives considered**:

- Hand-rolled YAML parser — REJECTED. Reinventing for no gain; `serde_yaml` is mature.
- Use `serde_yaml::Value` field-access instead of typed `Deserialize` — VIABLE but more brittle. Typed `Deserialize` with `#[serde(default)]` on optional fields handles schema evolution within Stack 2.x cleanly.

## R4 — `*.cabal` DSL parsing (multi-stanza Q2 union)

**Decision**: The Cabal DSL is line-format with indentation-based stanza scoping. Each stanza (`library`, `executable <name>`, `test-suite <name>`, `benchmark <name>`, `foreign-library <name>`) opens on a line with the stanza keyword (and optional name for non-library stanzas) and contains indented `<field>: <value>` lines until either a blank line or the next stanza opener.

Top-level fields (outside any stanza): `name:`, `version:`, `license:`, `author:`, `synopsis:`, `description:`, etc.

The reader's parse pipeline:

1. Extract top-level `name:` and `version:` via the regex pair `(?m)^name:\s*(\S+)` + `(?m)^version:\s*(\S+)`.
2. Locate each stanza opener via regex `(?m)^(library|executable|test-suite|benchmark|foreign-library)(?:\s+(\S+))?\s*$`.
3. For each stanza, capture the indented block until the next stanza opener or EOF, then extract its `build-depends:` and `build-tool-depends:` blocks.
4. Per-stanza `build-depends:` content is a comma-separated list (potentially multi-line via continuation) of `<pkg> [<version-range>]` entries. Reuse the same range-parsing regex from R2.
5. Q2 union: accumulate ALL stanzas' dep atoms with per-stanza `lifecycle_scope` (Runtime for `library`/`executable`; Development for `test-suite`/`benchmark`/`build-tool-depends`).
6. Most-binding-scope wins on name collision per Q2: when the same dep appears in multiple stanzas, runtime wins over development.

**Regex set**:

- `CABAL_NAME_RE` — `(?m)^name:\s*(\S+)`
- `CABAL_VERSION_RE` — `(?m)^version:\s*(\S+)`
- `CABAL_STANZA_RE` — `(?m)^(library|executable|test-suite|benchmark|foreign-library)(?:\s+(\S+))?\s*$`
- `CABAL_BUILD_DEPENDS_RE` — `(?ms)^\s+build-depends:\s*([^\n][\s\S]*?)(?:^\s+\w|^\w|\z)` (multiline, captures the indented block following `build-depends:` until the next field or stanza opener)
- `CABAL_BUILD_TOOL_DEPENDS_RE` — same shape but matching `build-tool-depends:`

**Hpack header detection** (Q3 + FR-015):

- `HPACK_HEADER_RE` — `(?m)^-- This file has been generated from package\.yaml by hpack version`

When this regex matches a `*.cabal`'s first ~5 lines AND a sibling `package.yaml` exists in the same directory, the reader emits a `tracing::warn!` diagnostic per FR-015.

**Rationale**: The Cabal DSL's line-format-with-indentation is regular enough for regex extraction. Full Cabal parsing requires the `Cabal` Haskell library — out of bounds per Principle I.

**Alternatives considered**:

- Full Cabal-format parser via FFI to the `Cabal` Haskell library — REJECTED (Principle I + would require Haskell runtime).
- Use Python's `cabal-helper-py` via a subprocess shell-out — REJECTED (network of subprocess complexity for no semantic gain; regex covers the documented patterns).
- Skip stanza discrimination entirely (single global `build-depends:` extraction) — REJECTED per Q2. Loses per-stanza lifecycle-scope tagging.

## R5 — Stack snapshot resolver placeholder (FR-005)

**Decision**: For each Stack project (detected via `stack.yaml` presence), emit ONE placeholder component per snapshot resolver. The resolver identifier comes from `stack.yaml`'s `resolver:` field (human-friendly: `lts-22.0` / `nightly-2024-01-15` / `ghc-9.6.4`). The snapshot SHA comes from `stack.yaml.lock`'s `snapshots[].completed.sha256` field; when `stack.yaml.lock` is absent, the placeholder version slot uses `unspecified`.

PURL shape: `pkg:generic/<resolver-prefix>-<resolver-id>@<sha-or-unspecified>` where `<resolver-prefix>` is `stackage` for `lts-*`/`nightly-*` and `ghc` for `ghc-*`. Examples:

- `pkg:generic/stackage-lts-22.0@<sha>`
- `pkg:generic/stackage-nightly-2024-01-15@<sha>`
- `pkg:generic/ghc-9.6.4@unspecified`

Annotations:

- `mikebom:source-type = "hackage-snapshot"`
- `mikebom:stackage-resolver = "<resolver-id>"` (exact identifier from `stack.yaml`)
- `mikebom:sbom-tier = "source"` (lockfile-derived) or `"design"` (when `stack.yaml.lock` absent)

**Snapshot expansion deferral**: Expanding the snapshot to its ~2500 individual packages would require fetching the snapshot manifest from `stackage.org` — incompatible with FR-012 (no network). Deferred to v1.1 via an opt-in `--expand-stackage-snapshots` flag (mentioned in spec Out-of-Scope). The placeholder approach keeps v1 fully offline + gives operators a clear identifier for downstream snapshot-correlation tools.

**Rationale**: The placeholder approach is the established pattern for "curated bundle whose contents we can't enumerate offline" — matches the milestone-141 Q1 OTP-runtime-libs over-emission posture (where mikebom emits `pkg:generic/<lib>@unspecified` placeholders for OTP runtime libs that don't appear in `rebar.lock`). The Stackage case is a more cohesive bundle (one identifier per resolver) but the same shape applies: emit something operator-visible rather than dropping it.

**Alternatives considered**:

- Network-fetch the snapshot manifest at scan time — REJECTED (FR-012 no-network).
- Bundle a snapshot-name-to-package-list catalog in mikebom — REJECTED. Catalog goes stale (new Stackage snapshots released weekly); maintaining a 2500-package-list per snapshot is a maintenance liability that scales poorly.
- Skip Stack snapshot resolvers entirely (only emit explicit extra-deps) — REJECTED per FR-005 + spec User Story 2.

## R6 — Constitution Principle V audit for the three new `mikebom:*` annotations

**Three new annotations introduced by this milestone**:

1. `mikebom:ghc-stdlib = "true"` (Q1, FR-014)
2. `mikebom:stackage-resolver = "<resolver-name>"` (FR-005)
3. `mikebom:hpack-source-detected` (NOT YET FORMALIZED — FR-015 emits a `tracing::warn!` diagnostic, NOT an SBOM-component-level annotation; clarify during implementation whether to also stamp a transparency annotation on emitted components, deferred decision)

**Per Constitution Principle V**, each new `mikebom:*` annotation must be audited against standards-native carriers in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 BEFORE introduction. Audit results:

### Audit: `mikebom:ghc-stdlib`

- **CDX 1.6 audit**: `component.scope` (required/optional/excluded) — REJECTED as carrier. Semantic is "is this dep required for the app" (lifecycle-scope axis); orthogonal to "is this an ecosystem stdlib member" axis. `component.evidence.identity.confidence` — REJECTED. No native field for "ecosystem stdlib membership."
- **SPDX 2.3 audit**: `Package.builtDate` / `Package.primaryPackagePurpose` (FILE/INSTALL/OPERATING-SYSTEM/...) — REJECTED. `OPERATING-SYSTEM` is close in spirit but specifically scoped to OS distros; misusing it for language stdlibs would conflict with the spec's intent. No fit.
- **SPDX 3 audit**: `software_softwarePurpose` enum — same shape as SPDX 2.3's `primaryPackagePurpose`. No fit.
- **Outcome**: `mikebom:ghc-stdlib = "true"` is a durable parity-bridge. Identical shape to milestone-141's `mikebom:otp-stdlib = "true"` (which had the same outcome). Documented in `docs/reference/sbom-format-mapping.md` Section I milestone-143 row.

### Audit: `mikebom:stackage-resolver`

- **CDX 1.6 audit**: `component.group` (vendor/grouping) — REJECTED. Stackage isn't the vendor; the resolver identifier is metadata about how packages were grouped at build time, not a vendor name. `component.evidence.distribution.url` — REJECTED. Not about distribution URL; the resolver is a curation identifier.
- **SPDX 2.3 audit**: `Package.sourceInfo` (free-text) — would technically fit but loses machine-readability. `Package.releaseDate` / `Package.builtDate` — temporal, wrong axis.
- **SPDX 3 audit**: same shape as SPDX 2.3. No fit.
- **Outcome**: `mikebom:stackage-resolver` is a durable parity-bridge. No native field carries "the curated-bundle identifier this component participated in." Documented in milestone-143 row.

### Audit: `mikebom:hpack-source-detected` (deferred decision)

- The FR-015 spec wording emits a `tracing::warn!` diagnostic — operator-facing console output, NOT an SBOM-component-level annotation. The audit only matters if we decide to ALSO stamp a transparency annotation on the affected `*.cabal`-derived main-module component.
- **Decision deferred to implementation phase**: assess during US3 test development whether operators benefit from an in-SBOM marker beyond the stderr warning. Default: stderr-only (no SBOM annotation), matching the milestone-005 "scan diagnostics emit to stderr" pattern.

**Rationale**: Principle V audit is the durable record-keeping that prevents future contributors from reinventing the same fields. Both confirmed-bridge annotations follow the established pattern (milestone-141 OTP-stdlib + milestone-142 scala-version-source).

## R7 — Multi-package project discovery

**Decision**: For multi-package Haskell projects, the reader discovers all `*.cabal` files in the source tree via `safe_walk` (per the existing walker pattern from milestone 114). Each discovered `*.cabal` becomes a main-module per FR-011 + FR-013. Same-PURL deps across sub-packages dedup via the standard `seen_purls: HashSet<String>` pattern.

The reader does NOT enumerate `cabal.project`'s `packages:` field for multi-package discovery — that field can use glob patterns (`./**/*.cabal`) that are easier to discover via direct filesystem walk. The `cabal.project` file's presence DOES trigger reader activation per FR-001 (alongside `*.cabal` and `stack.yaml*`).

**Stack equivalent**: `stack.yaml`'s `packages:` field follows the same shape; v1 ignores its content and relies on the filesystem walk.

**Rationale**: Simpler than parsing `cabal.project` / `stack.yaml`'s `packages:` field syntax + matches the milestone-142 Q2 Scala union-discovery approach (which also unioned multiple discovery surfaces). Filesystem walk catches every `*.cabal` regardless of declaration source.

**Alternatives considered**:

- Parse `cabal.project`'s `packages:` field and resolve glob patterns — REJECTED. The Cabal-project DSL's glob syntax has its own quirks (`./*` vs `./**/*` vs explicit lists); reinventing this when filesystem walk is the source-of-truth gains nothing.
- Only emit one main-module per root `*.cabal` (skip sub-package `*.cabal`s) — REJECTED. Cardano-style projects with 10+ sub-packages would silently lose 9 main-modules; spec FR-011 + SC-009 require one main-module per local package.

## R8 — `mikebom:source-type` prefix convention

**Decision**: Haskell-derived components carry `mikebom:source-type` annotation values prefixed `hackage-`:

- `hackage-freeze` — from `cabal.project.freeze` exact-pin entry
- `hackage-stack-lock` — from `stack.yaml.lock` explicit extra-deps
- `hackage-snapshot` — Stackage placeholder per FR-005
- `hackage-cabal-design` — from `*.cabal` `build-depends:` design-tier fallback
- `hackage-main-module` — per-package main-module

**Rationale**: Per the established cross-milestone convention (`kmp-` milestone 122, `pub-` milestone 137, `composer-` milestone 138, `cocoapods-` milestone 139, `hex-` milestone 140, `erlang-` milestone 141, `scala-` milestone 142), each new reader gets its own prefix. Haskell gets `hackage-` (matches the registry name + the PURL type).

**Cross-reader interaction**: Haskell projects don't collide with other ecosystems' PURLs (Hackage is the only ecosystem emitting `pkg:hackage/`), so the dedup-via-`seen_purls` mechanism doesn't fire across reader boundaries. The prefix preserves provenance for cross-reader debugging in the rare polyglot project (e.g., a project with both `*.cabal` and `Cargo.toml`).

## R9 — Regex compile-once via `std::sync::OnceLock`

**Decision**: All regex patterns used by `haskell.rs` are compiled once via `static REGEX_NAME: OnceLock<Regex> = OnceLock::new();` at module scope. Pattern compile is amortized across every scan invocation per the milestone-141 R7 + milestone-142 R8 precedent.

**Critical reminder**: regex declarations inside loops or inside functions called from loops trigger `clippy::regex_creation_in_loops` (caught empirically in milestone 141, hit again preemptively in milestone 142). Hoist `OnceLock<Regex>` to function-top-level OR module-level static — NEVER inside the loop body.

**Rationale**: Same as milestone 141/142. The reader's parse helpers are invoked once per discovered artifact (potentially many in multi-package projects), and per-call regex compilation is wasteful. The pattern is established across milestones 069, 137-142.

## R10 — Byte-identity SBOM golden preservation (SC-004)

**Decision**: A source tree containing none of `*.cabal` / `cabal.project*` / `stack.yaml*` files MUST produce an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan. Gated by the existing 14-ecosystem regression suite (`mikebom-cli/tests/cdx_regression.rs`, `spdx_regression.rs`, `spdx3_regression.rs`).

**Rationale**: Same as milestones 141/142. Non-Haskell projects are the vast majority of scans; any unintentional output drift would break every existing golden.

**Memory note**: Per persistent memory `feedback_cross_host_goldens.md`, goldens are already cross-host-byte-identical. No new golden files are introduced.

## R11 — Walker integration: `*.cabal` + `cabal.project*` + `stack.yaml*` discovery

**Decision**: `safe_walk` from `mikebom-cli/src/scan_fs/walk.rs` (milestone 114) discovers the five artifact types. The reader filters by:

- file name `*.cabal` (extension match)
- file name `cabal.project` OR `cabal.project.freeze` (literal match)
- file name `stack.yaml` OR `stack.yaml.lock` (literal match)
- file name `package.yaml` (literal match — for Q3 Hpack-detect; the reader does NOT parse this file, only detects its presence alongside a Hpack-generated `*.cabal`)

Standard excludes apply: `.git/`, `dist-newstyle/`, `dist/`, `.stack-work/`, `node_modules/`. The reader's own `should_skip_descent` helper extends these with Haskell-specific dirs.

**Rationale**: The walker already supports arbitrary-depth artifact discovery (cargo's `Cargo.toml`, npm's `package.json`, maven's `pom.xml` all use this pattern). Haskell's `*.cabal` follows the same pattern.

**Specific exclude rationale**:

- `dist-newstyle/` — cabal-install 2.4+ build cache; contains a generated `plan.json` + per-component build artifacts. Excluding it prevents the reader from scanning generated artifacts.
- `dist/` — legacy cabal-install build cache (pre-2.4). Same rationale.
- `.stack-work/` — Stack's per-project build cache. Contains per-snapshot lockfile copies + extracted package source. Excluding it prevents double-counting.

## Summary table

| # | Decision | Inherits | Risk |
|---|---|---|---|
| R1 | `pkg:hackage/` PURL shape + `pkg:generic/<resolver>@<sha>` snapshot placeholder | new (purl-spec hackage-definition + milestone-141 placeholder pattern) | none |
| R2 | `cabal.project.freeze` line-format regex parsing | new | low — format is fully regular |
| R3 | `stack.yaml.lock` YAML via `serde_yaml` + Q3-style content-shape gate | milestone 142 Q3 | low — YAML schema stable since Stack 2.1 |
| R4 | `*.cabal` multi-stanza regex extraction + Q2 union + most-binding-scope precedence | new (Q2 clarification + milestone-141 R3 keyword-family precedent) | low — Cabal DSL is line-format-regular |
| R5 | Stack snapshot placeholder (one per resolver) | new (FR-005 — preserves FR-012 no-network) | low — operator-visible identifier preserved |
| R6 | Principle V audit for 2 confirmed new annotations + 1 deferred | milestone 141 + 142 Principle V audit pattern | none |
| R7 | Multi-package discovery via filesystem walk | milestone 141 + 142 union discovery | low |
| R8 | `hackage-*` source-type prefix | milestone 122+137-142 | none |
| R9 | OnceLock regex compile pattern | milestone 069+137-142 | none — empirical lesson from 141+142 informs hoisting placement |
| R10 | Byte-identity SBOM golden | milestone 002+ (every reader) | low — gated by existing regression suite |
| R11 | safe_walk discovery + standard excludes | milestone 114 | none |

All NEEDS CLARIFICATION resolved. Phase 1 ready.
