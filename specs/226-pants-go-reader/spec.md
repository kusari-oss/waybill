# Feature Specification: Pants Go reader

**Feature Branch**: `226-pants-go-reader`
**Created**: 2026-08-03
**Status**: Draft
**Input**: User description: "go.sum pants support"

## User Scenarios & Testing *(mandatory)*

Pants monorepos with Go code declare their dependency roots via
`go_mod(name="mod")` targets in `BUILD` files (usually at
`3rdparty/go/BUILD`), pointing to a directory containing `go.mod` +
`go.sum`. Individual Go packages are declared via
`go_package(name="pkg")` targets colocated with `.go` source files,
and third-party imports via `go_third_party_package(name="foo",
import_path="example.com/foo")`. Waybill's existing Go reader
(m053+m055+m160+m161) already parses `go.sum` files from any layout
— including Pants's `3rdparty/go/go.sum` — and emits
`pkg:golang/*` components with correct sha1 fingerprints and
dependency edges. What it does NOT do today is expose the Pants
target-address ownership that lets operators trace which
`go_binary` or `go_package` targets pull each dependency.

The Pants Go backend also pins its toolchain in `pants.toml`
`[golang]` via `expected_version = "1.21"` (minimum version guard)
and `min_dot_version = "1.21"`. Even though "expected" is a
minimum-version signal rather than a strict pin, downstream
compliance stakeholders reasonably ask "what Go version did Pants
audit this repo against" — the `expected_version` pin is the
answer they want.

This feature adds a Pants-aware Go enrichment layer that walks
BUILD files under the scan root, extracts `go_binary` /
`go_package` / `go_third_party_package` / `go_mod` target
declarations, attaches a new `waybill:pants-target=<address>`
annotation (catalog row C146) to any matching `pkg:golang/*`
component the existing Go reader emitted, AND parses
`pants.toml` `[golang]` for `expected_version` — emitting a
design-tier `pkg:generic/go@<version>` component analogous to
m225's shellcheck/shfmt/shunit2 tool pins.

### User Story 1 - Attach Pants target ownership to Go modules (Priority: P1) 🎯 MVP

An operator scans a Pants Go monorepo. Every `pkg:golang/*`
component the Go reader emits from `3rdparty/go/go.sum` gains a
`waybill:pants-target=<address>` annotation naming the Pants target
that declares it — either the `go_mod(name="mod")` owning the
go.sum file, or a specific `go_third_party_package(import_path=...)`
target when one exists for that import path.

**Why this priority**: Delivers immediate value — compliance and
security teams can now filter `waybill sbom scan` output by Pants
target (e.g., "which deps does the `//cmd/frontend:frontend`
binary pull?") without cross-referencing BUILD files manually.
Also establishes the enrichment pattern for future Pants-aware
Go milestones (per-binary reachability, per-target vuln triage).

**Independent Test**: Given a Pants repo with
`3rdparty/go/BUILD` declaring `go_mod(name="mod")` and
`3rdparty/go/go.sum` containing three third-party entries,
`waybill sbom scan` emits three `pkg:golang/*` components, each
carrying `waybill:pants-target=3rdparty/go:mod`.

**Acceptance Scenarios**:

1. **Given** a Pants Go repo with `3rdparty/go/BUILD` declaring
   `go_mod(name="mod")` and `3rdparty/go/go.sum` containing three
   third-party module entries, **When** operator runs
   `waybill sbom scan`, **Then** the emitted SBOM contains three
   `pkg:golang/*` components, each with a
   `waybill:pants-target=3rdparty/go:mod` annotation on top of
   the existing sha1 hash + version fields.
2. **Given** a Pants repo where `3rdparty/go/BUILD` ALSO
   declares `go_third_party_package(name="cobra",
   import_path="github.com/spf13/cobra")` (an explicit target for
   one specific dep), **When** operator runs `waybill sbom scan`,
   **Then** the `pkg:golang/github.com/spf13/cobra@<ver>`
   component's `waybill:pants-target` annotation contains BOTH
   `3rdparty/go:mod` AND `3rdparty/go:cobra`, comma-separated,
   lexically sorted.
3. **Given** a Pants repo with a `cmd/frontend/BUILD` declaring
   `go_binary(name="frontend", main=".")`, **When** operator runs
   `waybill sbom scan`, **Then** the emitted SBOM contains a
   `pkg:golang/*` main-module component (matching the repo's
   `go.mod` module path) with `waybill:pants-target` containing
   `cmd/frontend:frontend`.

---

### User Story 2 - Inventory the pinned Go toolchain (Priority: P2)

An operator's `pants.toml` includes `[golang]
expected_version = "1.21"`. The emitted SBOM contains a
design-tier `pkg:generic/go@1.21` component with
`waybill:source-file=pants.toml`, so downstream compliance
tooling can answer "which Go version audited this repo".

**Why this priority**: Second-order value — the toolchain
inventory is useful only if the operator has already gained the
per-target enrichment from US1. Also, many Pants Go repos rely
on Pants defaults (no `expected_version` pin), so this story's
payoff is smaller than US1's.

**Independent Test**: Given a Pants repo with `pants.toml`
containing `[golang] expected_version = "1.21"`,
`waybill sbom scan` emits one `pkg:generic/go@1.21` component
with `waybill:sbom-tier=design` and `waybill:source-file=pants.toml`
annotations.

**Acceptance Scenarios**:

1. **Given** a Pants Go repo with `pants.toml` declaring
   `[golang] expected_version = "1.21"`, **When** operator runs
   `waybill sbom scan`, **Then** the emitted SBOM contains
   `pkg:generic/go@1.21` with `waybill:sbom-tier=design` and
   `waybill:source-file=pants.toml`.
2. **Given** a Pants repo with `pants.toml` present but NO
   `[golang]` section, **When** operator runs `waybill sbom
   scan`, **Then** no Go toolchain component is emitted by this
   reader.
3. **Given** a Pants repo with `[golang]` declared but no
   `expected_version` key (operator relies on Pants default),
   **When** operator runs `waybill sbom scan`, **Then** no Go
   toolchain component is emitted (waybill declines to hardcode
   Pants's default Go version — same policy as m225's shellcheck
   handling).

---

### User Story 3 - Distinguish first-party from third-party packages (Priority: P3)

Components corresponding to `go_package` targets (first-party
code) carry `waybill:pants-target` values naming their `go_package`
addresses, while components from `go_third_party_package` or
implicit `go_mod` ownership carry those addresses. Operators can
filter first-party from third-party with a simple jq query on the
annotation prefix.

**Why this priority**: Refinement, not primary value. Without US3,
operators still get target attribution (US1) — they just get it
without the first-vs-third-party split that helps them prioritize
vuln triage.

**Independent Test**: Given a Pants repo with BOTH
`cmd/frontend/BUILD` (declaring `go_binary` + `go_package` for
the main module) AND `3rdparty/go/BUILD` (declaring `go_mod` for
third-party deps), `waybill sbom scan` emits distinct
annotation values on first-party vs third-party components.

**Acceptance Scenarios**:

1. **Given** a Pants Go repo with both `cmd/foo/BUILD` (declaring
   `go_binary(name="foo", main=".")` + `go_package(name="pkg")`)
   AND `3rdparty/go/BUILD` (declaring `go_mod(name="mod")`),
   **When** operator runs `waybill sbom scan`, **Then** the
   main-module component's `waybill:pants-target` contains
   `cmd/foo:foo` AND/OR `cmd/foo:pkg`; third-party components'
   `waybill:pants-target` contains `3rdparty/go:mod`.

---

### Edge Cases

- **BUILD file references a `go_third_party_package` whose
  `import_path` does NOT appear in any `go.sum`**: no component
  is emitted for the missing dep; INFO log notes the target
  declared an import path with no matching go.sum entry.
- **BUILD file `go_binary(main=".")` points at a package that
  doesn't correspond to any waybill-emitted main-module
  component**: WARN log names the target; no annotation is
  attached (nothing to attach to). Scan does not abort.
- **Multiple `go_mod` targets in the same repo** (multi-module
  Go monorepo): each `go_mod` target's directory is a distinct
  ownership root; components from `<dir>/go.sum` carry the
  corresponding `<dir>:<name>` annotation.
- **`go_package` and `go_third_party_package` targets for the
  same import path** (rare): the annotation contains BOTH target
  addresses, comma-separated + lexically sorted (same SC-006
  dedup contract as m225).
- **`pants.toml` `[golang] expected_version = "1.21.5"` (patch
  version)**: the annotation carries the exact operator-provided
  string; waybill does NOT normalize to major.minor.
- **`pants.toml` `[golang]` has BOTH `expected_version` AND
  `min_dot_version`**: only `expected_version` is emitted (v1
  scope); `min_dot_version` deferred.
- **Non-Pants Go repos** (no `BUILD` files with Go targets AND
  no `pants.toml` with a `[golang]` section): the existing Go
  reader's output is byte-identical to pre-feature-226 goldens
  (SC-003).
- **Pants BUILD file with `go_package(sources=...)` glob syntax**:
  waybill parses the `name=` kwarg for the address; the `sources=`
  glob is not consulted (waybill's Go reader already discovers
  `.go` files directly via `go.sum`).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The reader MUST walk `BUILD` files under the scan
  root using the existing `safe_walk` infrastructure (respects
  symlink cycle guards, `--exclude-path`, depth limits) — same
  discovery path as m225's pants_shell reader.
- **FR-002**: The reader MUST extract the following Pants Go
  target-declaration shapes from each discovered `BUILD` file:
  `go_binary(name=..., main=...)`,
  `go_package(name=..., **_)`,
  `go_third_party_package(name=..., import_path=...)`,
  `go_mod(name=...)`.
- **FR-003**: After the existing Go reader completes, the pants_go
  enrichment pass MUST inject a `waybill:pants-target=<address>`
  annotation into every `pkg:golang/*` component whose module
  path or import path corresponds to a declared Pants target.
  Multiple matching targets MUST merge into a single annotation
  value, comma-separated and lexically sorted.
- **FR-004**: For each `pkg:golang/*` component whose module
  path lies under a `go_mod`-declared directory (typically
  `3rdparty/go/`), the annotation MUST include the owning
  `go_mod` target address (e.g., `3rdparty/go:mod`).
- **FR-005**: For `pkg:golang/*` components whose PURL matches a
  `go_third_party_package(import_path=...)` declaration, the
  annotation MUST include that specific target address in
  addition to any `go_mod` owner.
- **FR-006**: For the main-module `pkg:golang/*` component (the
  one whose module path matches the repo's `go.mod`), the
  annotation MUST include every `go_binary(main=...)` target
  address whose resolved `main=` path (absolute path under the
  BUILD file's directory) matches the component's
  `source_path.parent()`, PLUS every `go_package` target whose
  declaring BUILD file's directory is a prefix of the
  main-module component's source-path directory.
- **FR-007**: The reader MUST read `pants.toml` at the scan root
  (when present) and extract `expected_version = "..."` from the
  `[golang]` subsystem section.
- **FR-008**: When `[golang] expected_version` is set to a
  non-empty string, the reader MUST emit ONE design-tier
  `pkg:generic/go@<version>` component with
  `waybill:source-file=pants.toml` (m080 row) and
  `waybill:sbom-tier=design`.
- **FR-009**: Per-file fail-open: any BUILD file with
  unrecoverable parse errors MUST log a WARN naming the file
  and be skipped; targets that parse successfully within an
  otherwise malformed BUILD file MUST still be applied; the
  whole scan MUST NOT abort.
- **FR-010**: The reader MUST emit exactly one INFO log line at
  scan end summarizing counts with these structured fields:
  `build_files_discovered=N`, `build_files_parsed_ok=N`,
  `build_files_skipped_corrupt=N`, `go_targets_found=N`,
  `components_annotated=N`, `toolchain_component_emitted=<0|1>`.
  When zero BUILD files are discovered AND no `pants.toml`
  `[golang]` section is present, the reader MUST NOT emit any
  log line (byte-identity guarantee).
- **FR-011**: Repos with no Pants BUILD files declaring Go
  targets AND no `pants.toml` `[golang]` section MUST produce
  byte-identical SBOM output to a pre-feature-226 scan of the
  same repo (SC-003 anchor).
- **FR-012**: The reader MUST NOT emit new `pkg:golang/*`
  components — it only enriches components the existing Go
  reader emits. If a `go_third_party_package` declares an
  import path with no corresponding go.sum entry, no synthetic
  component is fabricated (Constitution Principle IX — no
  fabrication of coordinates without ground-truth source).

### Non-Functional Requirements

- **NFR-001**: Enrichment pass adds under 100 ms to scan runtime
  on a Pants Go monorepo with 100 BUILD files and 500
  `pkg:golang/*` components.
- **NFR-002**: Reader adds zero cost on repos without any Pants
  BUILD files AND without `pants.toml` (early-return once the
  walker's first pass finds no candidates).

### Key Entities

- **Pants Go target declaration**: one `go_binary` / `go_package` /
  `go_third_party_package` / `go_mod` invocation in a BUILD file.
  Carries a `name` (target address suffix) and optionally an
  `import_path`, a `main=` path, or (for `go_mod`) an implicit
  same-directory `go.mod` + `go.sum` reference.
- **Pants target address**: canonical `<dir>:<name>` string
  (e.g., `3rdparty/go:mod`, `cmd/frontend:frontend`).
- **Go import path**: canonical Go module path (e.g.,
  `github.com/spf13/cobra`). Matches the module segment of
  `pkg:golang/<module-path>@<version>` PURLs.
- **go_mod ownership root**: the directory containing a
  `go_mod`-declaring BUILD file. All `pkg:golang/*` components
  whose sha1-derived source path lies under that directory
  inherit the `go_mod` target's address.
- **Toolchain pin**: `expected_version = "..."` value inside
  `pants.toml` `[golang]` section.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Given a synthetic Pants Go repo with 3 third-party
  entries in `3rdparty/go/go.sum` and a `3rdparty/go/BUILD`
  declaring `go_mod(name="mod")`, `waybill sbom scan` emits 3
  `pkg:golang/*` components each carrying
  `waybill:pants-target=3rdparty/go:mod`. (US1 gate)
- **SC-002**: Given `pants.toml` with `[golang] expected_version = "1.21"`,
  `waybill sbom scan` emits exactly one
  `pkg:generic/go@1.21` design-tier component. (US2 gate)
- **SC-003**: Scanning a Go repo without any Pants BUILD files
  AND without `pants.toml` `[golang]` produces byte-identical
  SBOM output to a scan of the same repo built from `main`
  before this feature landed.
- **SC-004**: Given a repo where `3rdparty/go/BUILD` declares
  BOTH `go_mod(name="mod")` AND
  `go_third_party_package(name="cobra", import_path="github.com/spf13/cobra")`,
  the `pkg:golang/github.com/spf13/cobra@<ver>` component's
  `waybill:pants-target` value contains `3rdparty/go:cobra,3rdparty/go:mod`
  (lex-sorted, comma-sep). (Multi-owner merge)
- **SC-005**: A BUILD file containing 3 valid Go targets and 1
  syntactically-broken target enriches the 3 valid targets'
  matching components + logs a WARN naming the broken target;
  the scan does not abort. (FR-009 fail-open gate)
- **SC-006**: A `go_third_party_package(import_path="example.com/nonexistent")`
  declaration with no matching go.sum entry produces no synthetic
  component; the missing import path is named in an INFO log. (FR-012 gate)

## Assumptions

- **BUILD file parsing reuses m225's regex-scoped extractor**:
  the Pants BUILD-DSL parser at
  `waybill-cli/src/scan_fs/package_db/pants_shell/build_dsl.rs`
  can be generalized (or the pattern re-applied) to recognize
  `go_binary` / `go_package` / `go_third_party_package` /
  `go_mod` in addition to shell targets. Same
  Principle-I-compliant approach: no embedded Python
  interpreter, no PyO3.
- **Enrichment pass runs AFTER the m191 reconciler (post-`reconcile_design_source_tiers`)**:
  the pants_go reader emits ZERO `pkg:golang/*` components of
  its own; it builds an `import_path → target_addresses` map
  from BUILD files, then runs an enrichment step that iterates
  the already-reconciled component set and injects annotations.
  Running post-m191 ensures the surviving component set is
  stable — annotations attach to the entries that will emit.
  Analogous to how m131 quality-metadata-backfill works.
- **Toolchain pin emission is a normal package_db reader
  path**: `pkg:generic/go@<version>` is emitted as a standard
  `PackageDbEntry` during `read_all`, mirroring m225's
  shellcheck/shfmt/shunit2 pattern.
- **New parity-catalog row C146 `waybill:pants-target`
  extension**: m225's C145 `waybill:pants-target` row was
  scoped to shell scripts (per-file-tier). Reusing the same
  row for `pkg:golang/*` components requires either (a)
  broadening C145's semantic description or (b) adding a new
  row C146 for Go-ecosystem application. Planning-time
  decision — likely broaden C145 since the annotation
  semantics are identical (target address(es), lex-sorted
  comma-sep), just with a different applicable-tier.
- **Only the scan-root `pants.toml` is consulted**: nested
  `pants.toml` files ignored. Matches m225 + m224 + m223.
- **`main=` path resolution for `go_binary`**: relative to the
  BUILD file's directory. `main="."` = same dir; `main="./cmd/foo"` =
  subdirectory. Waybill maps this to the main module's package
  directory to attach the annotation.
- **Fixtures use synthetic package names**:
  `github.com/waybill-fixture/*` per memory
  `feedback_fixture_synthetic_package_names`.

## Dependencies

- **Milestone 054** (`safe_walk`): reused for BUILD-file discovery.
- **Milestone 191** (PURL-level reconciler): no interaction. The
  enrichment pass runs on `pkg:golang/*` components already
  merged by m191, appending annotations without changing
  identity.
- **Milestone 225** (pants_shell reader): the regex BUILD-DSL
  extractor pattern is reused. The C145 catalog row is either
  broadened OR a sibling C146 is added — decision at plan time.
- **Milestone 053+055+160+161** (Go reader): the source of the
  `pkg:golang/*` components this reader enriches. No changes to
  the Go reader's API expected.

## Out of Scope

- **`go_source` / `go_test` targets**: these are file-level
  target types not commonly used in Pants Go (Pants prefers
  `go_package`). Deferred until operator demand emerges.
- **Cross-repo Pants workspaces**: per-scan-root scope only.
- **`min_dot_version` from `pants.toml` `[golang]`**: v1 emits
  only `expected_version`. `min_dot_version` (a version-guard
  lower bound) is a distinct semantic and would emit a distinct
  component; deferred until demand emerges.
- **`pants.toml` `[go-test]` / `[go-vet]` / other Go-adjacent
  subsystem sections**: v1 only handles `[golang]`.
- **Pants's built-in golang goal invocations** (`pants
  package ::` etc.): waybill doesn't shell out to `pants`;
  it reads only static BUILD + config files.
- **Custom user-defined Go target types** (via `pants.toml`
  plugin registration): only the 4 built-in types are recognized.
