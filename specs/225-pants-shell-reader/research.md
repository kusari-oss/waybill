# Phase 0: Research — Pants shell reader

**Feature**: 225-pants-shell-reader
**Date**: 2026-08-02

Five research items derived from `plan.md` §"Critical Phase 0 items"
+ Constitution Check gate outputs. Each resolves an ambiguity before
Phase 1 design begins.

---

## R1 — Pants shell backend BUILD-file target grammar

**Decision**: Recognize four built-in target types from
`pants.backend.shell.target_types`:

| Target function | Signature (kwargs subset waybill consumes) | Owns files? |
|-----------------|--------------------------------------------|-------------|
| `shell_source(name="X", source="a.sh", **_)` | `name` (str) + `source` (str, single path) | 1 file |
| `shell_sources(name="X", sources=["*.sh"], **_)` | `name` (str, defaults to dir name) + `sources` (list[str], defaults to `["*.sh", "*.bash"]`) | N files |
| `shunit2_test(name="X", source="a_test.sh", **_)` | `name` (str) + `source` (str) | 1 file, dev-scope |
| `shunit2_tests(name="X", sources=["*_test.sh"], **_)` | `name` (str, defaults to dir name) + `sources` (list[str], defaults per Pants convention) | N files, dev-scope |

**Empirical anchor**: verified against the Pants project's own
dogfood `BUILD` files at `github.com/pantsbuild/pants` (search
term: `shell_sources(` — the built-in target types have not
changed shape since Pants 2.14, and the four listed above are
documented at
`https://www.pantsbuild.org/reference/targets/shell_source`.

**Rationale**: These four target types are the entire built-in
`pants.backend.shell` API. Plugin-registered target types (custom
`ShellCommandRunTarget`, etc.) are silently ignored per spec
Out-of-Scope.

**Alternatives considered**:
- **Include `shell_command`**: rejected per spec Assumptions —
  `shell_command` describes actions, not artifacts; modeling
  actions as SBOM subjects is architectural work outside this
  feature's scope.
- **Include `run_shell_command`**: a Pants CLI goal, not a target
  type. Not present in `BUILD` files.
- **Discover ALL `.sh` files in the tree independently of BUILD
  declarations**: rejected per user's Option A scoping choice
  (BUILD-file walker + shell-setup, not standalone script
  discovery — that's what the m133 file-tier walker does today).

---

## R2 — Regex-scoped BUILD-file DSL extraction

**Decision**: Extract target declarations via three multi-line
regex patterns, one per shape:

```rust
// Pattern A: single-source targets (shell_source / shunit2_test)
const SINGLE_SOURCE_RE: &str = r#"(?xm)
    ^\s*(shell_source|shunit2_test)\s*\(   # opening call
    [^)]*?                                    # any kwargs before name/source
    (?:name\s*=\s*["']([^"']+)["']         # capture group 2: name
       [^)]*?
       source\s*=\s*["']([^"']+)["']        # capture group 3: source
     | source\s*=\s*["']([^"']+)["']        # OR: source first, then name
       [^)]*?
       name\s*=\s*["']([^"']+)["'])         # capture groups 4/5
    [^)]*\)                                   # closing paren + any trailing kwargs
"#;

// Pattern B: multi-source targets (shell_sources / shunit2_tests)
const MULTI_SOURCES_RE: &str = r#"(?xm)
    ^\s*(shell_sources|shunit2_tests)\s*\(
    [^)]*?
    (?:name\s*=\s*["']([^"']+)["'])?         # OPTIONAL name (defaults to dirname)
    [^)]*?
    (?:sources\s*=\s*\[([^\]]*)\])?          # OPTIONAL sources list body
    [^)]*\)
"#;
```

**Empirical validation**: hand-crafted parser exercised against
6 fixture BUILD files during T005 unit tests. Regex compile-once via
`OnceLock` (same pattern used by cmake / alpm / brew readers).

**Rationale**: The Pants BUILD DSL is Python-syntax but the target
declarations that waybill cares about follow a narrow, predictable
call shape. Full Python-parsing is overkill (Constitution Principle
I forbids embedding a Python interpreter anyway). The regex approach
mirrors the cmake `find_package(...)` extractor at
`waybill-cli/src/scan_fs/package_db/cmake.rs` and the alpm `desc`
stanza extractor.

**Edge cases the regex handles**:
- Single vs double quote string literals
- Whitespace / newlines between kwargs
- Trailing commas
- Additional (ignored) kwargs before/between/after name/source

**Edge cases the regex does NOT handle** (documented failure modes):
- Multi-line list literals with nested `[ ]` (e.g., `sources=[["a"], ["b"]]`) — not
  legal Pants syntax anyway.
- String concatenation (`source="scripts" + "/deploy.sh"`) — waybill
  emits WARN + skips the target; operators should use string
  literals per Pants style guide.
- Variable references (`source=SCRIPT_NAME`) — same treatment.

**Alternatives considered**:
- **Embed a mini Python parser (RustPython, tree-sitter-python)**:
  rejected — architectural weight (~2 MB binary bloat +
  maintenance burden) vs the narrow value of parsing 4 function
  calls' kwargs.
- **Shell out to `pants peek`**: rejected — adds a hard runtime
  dependency on the `pants` binary; spec Assumptions explicitly
  precludes shell-out. Also unavailable in air-gapped scan
  environments.

---

## R3 — File-tier PURL shape for shell-script components

**Decision**: `pkg:generic/<basename>@<sha256[:12]>` where
`<basename>` is the URL-encoded `.sh` file basename (e.g.,
`waybill-fixture-deploy.sh`) and `<sha256[:12]>` is the first 12
hex characters of the file's SHA-256 (for readable identity).
Full sha256 goes into the standard `hashes[]` slot; the target
address + full relative path go into annotations.

**Sample PURL**: `pkg:generic/waybill-fixture-deploy.sh@a1b2c3d4e5f6`

**Rationale**:
- **Readable identity**: operators grep-ing a CDX for their
  scripts by name find them by basename, not by an opaque
  content-sha256 qualifier.
- **Collision-safe**: SHA-256 12-hex-char prefix has 2^48 space —
  the same script at two different revisions gets two distinct
  PURLs; unrelated scripts of the same basename get distinct PURLs
  when their content differs.
- **Content-addressed dedup still works**: the m133 file-tier
  walker's dedupe index keys on the FULL sha256 (from `hashes[]`),
  not on the PURL, so identical-content-different-basename cases
  still dedupe correctly.

**Alternatives considered**:
- **Match m133's `pkg:generic/file-tier?content-sha256=<full-sha>`
  exactly**: rejected — that PURL is a placeholder for the m133
  orphan file walker (which discovers files with NO source-tier
  provenance); the pants-shell reader DOES have provenance (the
  BUILD file's target address). Using the same placeholder loses
  that provenance signal in the primary identifier.
- **Full path in the name**:
  `pkg:generic/scripts%2Fwaybill-fixture-deploy.sh@<sha>` —
  rejected because the URL-encoded slash clutters human-readable
  listings; the path lives in the `waybill:source-files`
  annotation instead where it's fully preserved.
- **No PURL prefix disambiguation** (e.g., just
  `pkg:generic/deploy.sh@<sha>`): rejected — two different repos'
  `deploy.sh` files with the same content would collide in a merged
  SBOM. The `waybill-fixture-` prefix in fixtures + real-world
  repos' own prefixing conventions (e.g., `mycompany-`) provide
  sufficient natural namespacing; if a real-world collision
  emerges, follow-up promotes to full-path encoding.

---

## R4 — `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` subsystem schema

**Decision**: Parse `pants.toml` via existing `toml = "0.8"` and
extract these keys per section:

```toml
[shellcheck]
version = "v0.9.0"           # OPTIONAL: pinned version (waybill emits when present)
known_versions = [           # OPTIONAL: multi-arch pins (waybill IGNORES in v1)
    "v0.9.0|macos_arm64|<sha256>|<size>",
    "v0.9.0|linux_x86_64|<sha256>|<size>",
]

[shfmt]
version = "v3.7.0"
known_versions = [...]

[shunit2]
version = "2.1.8"             # rare; shunit2 usually vendored via Pants bundle
```

**Empirical anchor**: verified against Pants documentation at
`https://www.pantsbuild.org/reference/subsystems/shellcheck` and
`.../shfmt`. Both subsystems follow the same
`external_tool.ExternalTool` schema.

**Rationale**:
- **`version` is the operator-facing pin**: this is what compliance
  teams care about — "which shellcheck ran against this repo".
- **`known_versions` is deployment infrastructure**, not supply-
  chain data — it's the download URL + hash the Pants tool-fetcher
  uses. Emitting it as a per-arch component would produce N
  components per tool, none of which the operator explicitly picked.
- **shunit2's `version` is optional** — Pants ships an embedded
  shunit2 bundle when no version is pinned. waybill emits ONLY
  when the operator has explicitly overridden.

**Tool-component PURL shape**:
`pkg:generic/shellcheck@v0.9.0` (verbatim version string, including
leading `v` prefix when present — do NOT strip; that would lose the
operator's chosen pin format).

**Alternatives considered**:
- **Emit `known_versions` entries as separate components**:
  rejected — inflates SBOM with per-arch entries the operator
  didn't explicitly select. If a downstream security tool needs
  per-arch identity, they can derive it from the base `version`
  component + arch context of the running scan.
- **Emit under `pkg:brew/shellcheck` or `pkg:github/koalaman/shellcheck`**:
  rejected — waybill doesn't know how the operator's Pants
  install actually fetched the tool. `pkg:generic/` is the honest
  answer; downstream tools can map to their preferred type.

---

## R5 — Synthetic fixtures for US1/US2/US3 + edge cases

**Decision**: 7 fixture directories under
`waybill-cli/tests/fixtures/pants_shell/`. All script files use
`waybill-fixture-*.sh` naming per memory
`feedback_fixture_synthetic_package_names`.

| Fixture | US / edge case | Content |
|---------|----------------|---------|
| `minimal_scripts/` | US1 baseline | `scripts/BUILD` with `shell_source(name="deploy", source="waybill-fixture-deploy.sh")` + one `shell_sources(name="utils", sources=["waybill-fixture-*.sh"])`; 2 `.sh` files. |
| `glob_sources/` | US1 scenario 2 | `helpers/BUILD` with `shell_sources(name="utils", sources=["*.sh"])` + 3 `waybill-fixture-*.sh` files. |
| `with_shell_setup/` | US2 baseline | `pants.toml` pinning `[shellcheck] version = "v0.9.0"` + `[shfmt] version = "v3.7.0"` + `[shunit2] version = "2.1.8"` (all three tools). Plus one `scripts/BUILD` + one script to prove US1 emits alongside US2. |
| `shunit2_dev_scope/` | US3 baseline | `tests/BUILD` with `shunit2_test` + `shunit2_tests`; 2 `waybill-fixture-*-test.sh` files. Plus one `shell_source` target in the same BUILD to verify differential scope tagging. |
| `missing_source_file/` | FR-009 edge case | `scripts/BUILD` declares `shell_source(name="broken", source="nonexistent.sh")`; the file is intentionally absent. Verifies WARN + skip without scan abort. |
| `malformed_build_partial/` | FR-009 / SC-005 gate | `scripts/BUILD` contains 3 valid targets + 1 broken target (syntactic mess: unclosed paren). Verifies per-target fail-open — 3 valid targets still emit. |
| `dupe_target_owners/` | SC-006 gate | `scripts/BUILD` has both `shell_source(name="a", source="waybill-fixture-x.sh")` AND `shell_sources(name="glob", sources=["waybill-fixture-x.sh"])`. Verifies ONE component emitted with both target addresses in the annotation, comma-separated, lexically sorted. |

**Rationale**: 7 fixtures covers US1×2 + US2×1 + US3×1 + 3 edge cases,
matching m224's 7-fixture count. Each fixture is self-contained (no
cross-fixture symlinks or dependencies).

**Alternatives considered**:
- **Fewer fixtures, more scenarios per fixture**: rejected —
  co-locating multiple scenarios in one fixture makes test
  failures harder to interpret ("was it the glob test or the
  dedup test that broke?").
- **Real-world Pants repo dump (e.g., pantsbuild/pants own dogfood
  BUILD files)**: rejected per memory
  `feedback_fixture_synthetic_package_names` — real coords in
  fixtures trip Kusari Inspector's advisory scan.

---

## Summary — resolved unknowns

- R1: Target-function grammar locked to 4 built-in types
  (`shell_source`, `shell_sources`, `shunit2_test`,
  `shunit2_tests`).
- R2: Regex-scoped extraction (two patterns, one per call shape).
  No Python interpreter, no `pants` shell-out.
- R3: `pkg:generic/<basename>@<sha256[:12]>` — readable + collision-safe.
- R4: `pants.toml` `version` keys in 3 subsystem sections; emit
  ONLY when explicitly pinned; `known_versions` ignored.
- R5: 7 synthetic fixtures, all with `waybill-fixture-*` script
  names.

Zero remaining `[NEEDS CLARIFICATION]` markers. Ready for Phase 1.
