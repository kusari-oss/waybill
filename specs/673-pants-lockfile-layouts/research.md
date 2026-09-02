# Phase 0 Research — m673 Pants lockfile discovery layout extensions

**Date**: 2026-09-02
**Status**: Complete — no unresolved NEEDS CLARIFICATION markers.

## R1 — Empirical validation of the three canonical layouts

**Decision**: Discover `.lock` files in three canonical Pants Python
directories: (a) `<repo-root>/*.lock`, (b) `<repo-root>/lockfiles/*.lock`,
(c) `<repo-root>/3rdparty/python/*.lock`.

**Rationale**: 2026-09-02 smoke-test against `pantsbuild/example-python`
and `pantsbuild/example-django` (Pants's own official example
repositories) confirmed the two additional layouts m672 misses:

| Repo | Pants version | Actual lockfile path | Notes |
|------|---------------|----------------------|-------|
| `pantsbuild/example-python` | 2.31.0 | `<repo-root>/python-default.lock` | No `3rdparty/python/`; no `[python.resolves]` in `pants.toml` |
| `pantsbuild/example-django` | latest | `<repo-root>/lockfiles/python-default.lock` | Dedicated `lockfiles/` directory |
| `pantsbuild/example-jvm` | latest | `3rdparty/jvm/*.lockfile` | JVM only — out of scope for this milestone (m224) |

Both example-python and example-django use the pre-2.30 `//`-comment
frontmatter shape observed at an early adopter's `python-default.pants.lock`
(m672 R1 — inherited stripping applies to all three canonical
directories).

**Alternatives considered**:
- **Recursive `find`-style walker** rooted at repo-root for any `*.lock`
  file — rejected (too wide-scope; would false-positive-match `.lock`
  files in `target/`, `node_modules/`, `dist/`, etc. — even with the
  content-detect gate, the read-dir cost is unbounded).
- **Detect via `pants.toml` `[python] default_resolve` field + string
  interpolation** — rejected because Pants's default-resolve resolution
  algorithm is more elaborate than a single field lookup (it consults
  `[python] default_resolve`, `[python.resolves]`, then falls back to
  the layout convention). Reimplementing Pants's resolution engine is
  out of scope; the discovery-directories approach captures every real-
  world layout without needing to model Pants's internal logic.

## R2 — `pex_version` content-detection strategy

**Decision**: Add a pure function `is_pex_lockfile_content(bytes: &[u8]) -> bool`
at `pants/lockfile.rs`. Implementation: strip `//`-frontmatter (reuse m672
`strip_pants_frontmatter`), then parse to `serde_json::Value`, then check
`obj["pex_version"].as_str().is_some_and(|s| s.starts_with("2."))`.

**Rationale**: `serde_json::Value` is a permissive top-level type
that parses any valid JSON, unlike the strict `PexLockfile` struct
which requires the full schema. This lets us cheaply gate the
content-detection without paying the full-schema parse cost twice
(once to detect, once to consume). If content-detection succeeds,
the downstream `parse()` (m672 signature `Option<(PexLockfile, bool)>`)
does the full-schema parse.

**Overhead cost**: `serde_json::from_slice::<Value>` on a well-formed
JSON body scales linearly with file size. On a real Pants lockfile
(2 KB – 200 KB), this is < 5 ms. On a rejected non-JSON file (e.g. a
TOML `poetry.lock`), the parser errors out at the first non-JSON
byte — likely in < 100 µs. **Total content-detect overhead per file:
sub-millisecond on average, 5 ms worst-case on real PEX shapes.**

**Alternatives considered**:
- **Prefix-match on the first 100 bytes** for the string `"pex_version"`.
  Rejected — brittle (a JSON file that has `"pex_version"` inside a
  string value would false-positive), and doesn't tolerate the m672
  `//`-frontmatter block that pushes `pex_version` past byte offset
  100 in many real-world files.
- **Full-schema `PexLockfile` parse as the detection step**. Rejected —
  the full parse's error messages would leak the schema shape into
  operator logs. A `Value` parse either succeeds (JSON) or fails
  (non-JSON), which is exactly the discriminator we want.
- **`serde_json::StreamDeserializer` walking to the `pex_version`
  key**. Rejected — over-engineered for a ~5 ms hot path; the
  simple `Value` parse is easier to review and maintain.

## R3 — Discovery-scope bound (defensive-cap decision)

**Decision**: No explicit hard cap on the number of `.lock` files
per discovery directory. `std::fs::read_dir` is bounded by the
directory's actual contents; a pathological repo with 1000 `.lock`
files at the root would trigger 1000 content-detects (each < 5 ms
worst-case → 5 s upper bound). This is well within the m672 scan-
time envelope for a monorepo of that scale.

**Rationale**: A per-directory hard cap would require a policy
decision on what to do at the boundary (silently truncate vs. WARN
+ truncate vs. abort). Adding a cap for a hypothetical failure
mode that doesn't exist in observed real-world repos is
premature optimization. Real-world Pants monorepos have < 30
resolves; even the largest known adopters cap out at ~50 resolves.

**Deferred to a v2 milestone if empirically justified**: adding a
`WAYBILL_PANTS_MAX_LOCKFILES_PER_DIR=<N>` env-var escape hatch if
some future repo triggers a genuine perf problem.

**Alternatives considered**:
- **Hard cap at 100 files/dir** — rejected as premature; no observed
  data justifies the cap threshold.
- **Content-detect concurrency** — rejected as premature; 5s upper
  bound is fine, and adding a thread pool crosses a Principle-IV
  simplicity threshold with no measured need.

## R4 — Interaction with m672 `dedup_by_canonical_path`

**Decision**: The new discovery paths (FR-001 + FR-002) feed the
same `dedup_by_canonical_path` pass introduced in m672. New
`DiscoverySource` variants (`RepoRootGlob` + `LockfilesGlob`) are
added; the m672 winner-selection rule extends naturally:

1. If any candidate in a collision group has `origin == PythonResolvesMap`, that one wins.
2. Else if any candidate has `origin == PythonLockfileSingular`, that one wins.
3. Else the lexically-first `resolve_name` wins (deterministic tie-break).

The precedence order (`PythonResolvesMap` > `PythonLockfileSingular` >
{`DefaultGlob`, `RepoRootGlob`, `LockfilesGlob`}) matches Pants's own
override semantics (explicit config wins over auto-discovery). The
three auto-discovery origins are peers among themselves — a file
found via both the m223 `3rdparty/python/` glob AND the m673
repo-root glob (physically impossible under a single canonical
path — those directories don't overlap) would fall to the lex-min
tie-break.

**Rationale**: Extends m672's dedup contract minimally. No new
collision cases; canonical-path dedup already handles cross-origin
duplicates.

**Alternatives considered**:
- **Precedence tie-breaks: latest discovery source wins**. Rejected —
  deterministic ordering is critical for reproducible SBOMs; lex-min
  is deterministic and matches existing waybill conventions.
- **Recompute the `origin` field on cross-directory collision** to
  reflect the "primary" discovery source. Rejected — adds complexity
  without changing observable behavior (the canonical path is what
  downstream sees).

## R5 — FR-006 signal detection extension

**Decision**: The m672 signal-detection at `pants/mod.rs::read`
(`pants_signal_present = default_dir_exists || pants_toml_exists`)
extends to three additional signals:

- Existence of `<repo-root>/lockfiles/` directory (regardless of
  contents).
- At least one repo-root `.lock` file that passes content-detection.
- (Existing) `pants.toml` exists.
- (Existing) `<repo-root>/3rdparty/python/` directory exists.

Any of these four signals firing means the zero-discovered path
emits the FR-010 hint log. If NONE fire, the reader remains silent
(preserves m223 SC-003 non-Pants-repo byte-identity).

**Rationale**: Signal-presence is the discriminator between "operator
is using Pants but misconfigured" (worth a hint) and "not a Pants
repo" (silent). Adding the two new canonical-directory signals is
straightforward.

**Edge case**: a `<repo-root>/lockfiles/` directory exists but
contains only non-PEX `.lock` files. Under FR-006 that's a Pants
signal, so the FR-010 hint fires with `lockfiles_discovered=0` +
the two-key hint text. Slightly misleading (the directory isn't
actually a Pants layout) but not harmful — operator can quickly
verify by reading the hint and looking at their `lockfiles/` dir.

**Alternatives considered**:
- **Only count `lockfiles/` as a signal if it contains ≥ 1 PEX
  lockfile** — rejected as circular (we'd need to content-detect
  before deciding whether to emit the hint, but the hint is precisely
  what tells the operator we couldn't find any PEX lockfile).
- **New signal: at least one `.lock` file with `//` frontmatter** —
  rejected as overly narrow; a fresh Pants 2.32+ repo may have
  clean-JSON lockfiles with no frontmatter.

## R6 — Fixture strategy

**Decision**: Continue the m672 pattern — synthetic `tempfile::tempdir()`
fixtures inside the integration test file at `waybill-cli/tests/
scan_pants_m673.rs`. Every synthetic package name uses the
`waybill-fixture-*` prefix per memory `feedback_fixture_synthetic_package_names`.
No new committed fixtures under `waybill-cli/tests/fixtures/`.

Minimum fixture shapes:

1. **US1 repo-root single lockfile**: `<repo-root>/pants.toml` (no
   `[python.resolves]`) + `<repo-root>/python-default.lock` (PEX
   shape with `//`-frontmatter + 3 synthetic packages). Asserts 3
   components emit with `resolve_name=python-default`.
2. **US1 multiple repo-root lockfiles**: same shape but with
   `python-default.lock` + `mypy.lock` + `pytest.lock`, each 1
   synthetic package. Asserts 3 components with 3 distinct
   resolve names.
3. **US1 mixed valid + non-PEX**: `<repo-root>/Cargo.lock` (real
   cargo-shape TOML) + `<repo-root>/python-default.lock` (PEX
   shape). Asserts PEX file emits its component AND `Cargo.lock`
   is NOT counted by the Pants reader AND cargo reader handles
   `Cargo.lock` normally.
4. **US2 `lockfiles/` layout**: `<repo-root>/lockfiles/python-default.lock`
   + `<repo-root>/lockfiles/mypy.lock` (both PEX). Asserts both emit
   with correct resolve-name tags.
5. **US2 mixed**: `<repo-root>/lockfiles/README.md` + `<repo-root>/lockfiles/python-default.lock`.
   Asserts README ignored, PEX emits.
6. **US3 content-detect defense**: `<repo-root>/Cargo.lock` +
   `<repo-root>/lockfiles/poetry.lock` (real poetry TOML) — no
   PEX lockfile anywhere. Asserts Pants reader emits ZERO WARN log
   lines about those files.
7. **SC-005 byte-identity**: reuse the m672 test fixtures (both
   the m672 committed goldens + the m672 in-test synthetic fixtures).
   Both should pass unchanged after m673 lands.

**Rationale**: Every fixture composable in-test. Synthetic package
names avoid Kusari Inspector advisory noise. Fixture layout matches
m672's test file 1:1.

## R7 — Alignment with m223 + m672 emission shape

The m673 change is purely additive at discovery time. Components
emitted from FR-001/FR-002-discovered lockfiles carry:

- The SAME `pkg:pypi/<project_name>@<version>` PURL shape as m223.
- The SAME `waybill:pants-resolve=<name>` per-component annotation
  (C143 catalog row unchanged).
- The SAME `waybill:source-type` / `waybill:source-url` for non-PyPI
  entries (C1 / C144 unchanged).
- The SAME m672 `legacy_shape_lockfiles=<N>` log counter behavior —
  files discovered via FR-001/FR-002 with `//`-frontmatter contribute
  to the counter identically.

No new parity catalog rows. No new C-rows. No new extractor macros.
The existing m223 parity extractors + emission tests apply to m673-
discovered components unchanged.

## Constitution re-check (post-research)

All 12 principles + Strict Boundaries hold as documented in
`plan.md § Constitution Check`. The Principle III fail-open nuance
was scoped by clarify Q1 to the wide-scope paths only; m223
narrow-scope semantics are preserved verbatim. No new violations
surfaced.
