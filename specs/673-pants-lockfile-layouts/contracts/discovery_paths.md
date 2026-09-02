# Contract — Pants Python lockfile discovery paths

**Feature**: 673-pants-lockfile-layouts
**Applies to**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs::discover_lockfiles`

## Purpose

Extend the m223 + m672 Pants Python lockfile discovery pipeline
to enumerate two additional canonical directories used by Pants
2.31+ default layouts, gated by a content-detection check that
avoids false-positive parses of non-PEX `.lock` files.

## The five discovery paths

Post-m673, `discover_lockfiles(scan_root)` enumerates candidates
from FIVE sources in this fixed order (order matters ONLY for
deterministic WARN log ordering; canonical-path dedup collapses
duplicates):

1. **Default glob** (m223): `<scan_root>/3rdparty/python/*.lock`.
   Non-recursive. Every `.lock` file appended verbatim as
   `origin: DefaultGlob`. NO content-detection — m223 semantics
   preserved (parse-and-WARN-on-failure).
2. **Legacy singular** (m223): `pants.toml` `[python].lockfile`.
   File path resolved relative to `scan_root`. Missing-on-disk
   WARNs; parse failure WARNs. `origin: PythonLockfileSingular`.
3. **Resolves map** (m672): `pants.toml` `[python.resolves]`
   bare-string entries. Non-bare-string entries WARN + skip;
   missing-on-disk entries WARN + skip. Parse failure WARNs.
   `origin: PythonResolvesMap`.
4. **Repo-root glob** (m673, NEW): `<scan_root>/*.lock` (immediate
   children of `scan_root` only — non-recursive). Every `.lock`
   file is content-detected via `is_pex_lockfile_content` per
   contract [content_detection.md](./content_detection.md).
   Files that FAIL content-detection are SILENT-skipped (per FR-004).
   Files that PASS become candidates with `origin: RepoRootGlob`.
5. **Lockfiles-directory glob** (m673, NEW): `<scan_root>/lockfiles/*.lock`
   (immediate children of `<scan_root>/lockfiles/` only — non-
   recursive). Content-detected identically to path 4. Files that
   PASS become candidates with `origin: LockfilesGlob`.

All five paths feed the same `dedup_by_canonical_path` pass (m672
FR-009 semantics unchanged).

## Behavioral contract

### C1 — Additive discovery (FR-001, FR-002)

The m673 changes are additive. Every m223 + m672 candidate
continues to be discovered exactly as it was pre-m673. The m673
change adds paths 4 + 5 to the union; it does NOT remove or
re-order paths 1–3.

### C2 — Non-recursive (FR-009)

Paths 4 + 5 are strictly non-recursive. `<scan_root>/lockfiles/team-a/foo.lock`
is NOT discovered by path 5 (which only walks the immediate
children of `<scan_root>/lockfiles/`). Recursive discovery is
deferred to a v2 milestone.

### C3 — Content-detect gate for wide-scope paths only (FR-003, FR-004)

Paths 4 + 5 apply `is_pex_lockfile_content` as a pre-parse gate.
Files that fail the gate are SILENT-skipped — NO WARN log line,
NO `lockfiles_skipped_corrupt` counter increment, NO false-positive
component emission. This prevents spam on repos that contain non-
PEX `.lock` files from other ecosystems (Cargo, Poetry, bun).

Paths 1–3 do NOT apply the content-detect gate. Parse failures on
those paths continue to emit m223's WARN-and-skip. Rationale (per
2026-09-02 clarify Q1): those paths are conventionally Pants-owned,
so a WARN there catches genuine operator mistakes rather than
being a false-positive on unrelated files.

### C4 — Dedup semantics (FR-005, inherited from m672)

Two candidates that resolve to the same canonical path (via
`std::fs::canonicalize`) are parsed exactly once. Winner-selection
extends the m672 rule:

1. Any candidate with `origin == PythonResolvesMap` wins.
2. Else any candidate with `origin == PythonLockfileSingular` wins.
3. Else lex-min `resolve_name` among the tied peers (DefaultGlob,
   RepoRootGlob, LockfilesGlob).

### C5 — Downstream reader non-interference (FR-007)

A `.lock` file silent-skipped by paths 4/5 remains available to
downstream readers (cargo, pip-poetry, bun, etc.) with byte-
identical file-content access. The Pants reader's content-detect
gate reads the file bytes but does NOT mutate or claim them.

### C6 — File-tier walker non-interference (FR-008)

A `.lock` file silent-skipped by paths 4/5 remains eligible for
file-tier emission via the m133 orphan walker + m671 source-tree
mode. "Silent-skip in Pants reader" ≠ "invisible to whole scan".

### C7 — FR-006 signal detection extension

The `pants_signal_present` flag at `pants/mod.rs::read` (m672 US3)
extends to include TWO new signals:

- `<scan_root>/lockfiles/` directory exists.
- At least one `<scan_root>/*.lock` file passes content-detection.

If ANY signal fires (existing `pants.toml` OR existing `3rdparty/python/`
OR new `lockfiles/` OR new content-detected repo-root PEX) AND
discovery found zero candidates, emit the m672 diagnostic INFO log
naming both supported override keys.

If NO signal fires, the reader remains silent (m223 SC-003 preserved).

## Test matrix

| Repo shape | Expected | Passes contract |
|---|---|---|
| `<root>/python-default.lock` (PEX 2.x, `//`-frontmatter) | Discovered via path 4, emits components | C1, C3 |
| `<root>/python-default.lock` (Pex 1.9) | Discovered via path 4, WARNs "unsupported version", zero components | C3 (content-detect gate rejects on `pex_version == "1.9.0"`; falls back to silent-skip on paths 4/5) |
| `<root>/lockfiles/python-default.lock` (PEX 2.x) | Discovered via path 5, emits | C1, C2 (non-recursive) |
| `<root>/lockfiles/team-a/mypy.lock` (PEX 2.x, but nested one level too deep) | NOT discovered | C2 non-recursive |
| `<root>/Cargo.lock` (TOML) | Silent-skip by path 4; cargo reader handles | C3, C5 |
| `<root>/lockfiles/poetry.lock` (TOML) | Silent-skip by path 5; pip reader handles | C3, C5 |
| `<root>/lockfiles/foo.lock` (PEX) + `<root>/3rdparty/python/foo.lock` (PEX, same file via symlink) | Parsed once, `origin: LockfilesGlob` wins on lex-min after canonicalize | C4 |
| No `pants.toml`, no `3rdparty/python/`, no `<root>/*.lock`, no `<root>/lockfiles/` | Reader silent (zero log lines) | C7 |
| No `pants.toml`, empty `<root>/lockfiles/` directory | Reader emits FR-010 hint log with `lockfiles_discovered=0` | C7 signal fired |

## Failure modes

- **`std::fs::read_dir` fails on `<scan_root>/lockfiles/`** (e.g. permission denied): reader logs `tracing::warn!` naming the path + error, continues with the other 4 paths. Matches m223 read-dir behavior at path 1.
- **`std::fs::read` fails on a candidate `.lock` file**: content-detect returns `false` (bytes-not-readable → not a valid PEX). No WARN (per FR-004 for paths 4/5). For paths 1–3, m223's WARN-and-skip continues to apply.
- **`std::fs::canonicalize` fails during dedup** (m672 behavior): candidate is dropped from the dedup group with a WARN. Unchanged from m672.

## Performance envelope

Two additional `read_dir` calls per scan + one content-detect per
`.lock` file at repo-root or under `lockfiles/`. Real-world upper
bound: 20 `.lock` files per directory × 5 ms content-detect worst
case = 100 ms total overhead. Well within m672 SC-007's implicit
per-reader budget (< 1 s per reader on 1 GB monorepos).
