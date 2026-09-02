# Contract — Pants FR-002 fallback integration

**Feature**: 674-uv-lock-reader
**Applies to**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs::read` + `uv/lockfile.rs::parse`

## Purpose

Define the hook-into-m673 mechanism per FR-002 + research.md §R3.
When the m673 Pants discovery pipeline finds a `.lock` file that
FAILS `pants::lockfile::parse` (PEX-JSON parse fails), the uv reader
gets a second attempt via a callback. This handles the case observed
at `lablup/backend.ai` in the m673 sweep: Pants monorepo with
`[python.resolves]` naming files that are UV-format TOML, not PEX
JSON.

## Design principles

- **Single source of truth for discovery**: the m673 Pants pipeline
  owns lockfile discovery (its 5 sources — default glob + m672
  singular + m672 map + m673 repo-root + m673 lockfiles-glob). The
  uv reader does NOT run a second discovery pass over the same
  directories.
- **Parser dispatch, not parser fusion**: `pants::lockfile::parse`
  and `uv::lockfile::parse` remain fully independent functions
  with distinct schemas. The dispatch happens in `pants/mod.rs::read`.
- **Preserve Pants context**: when the uv reader emits components
  from a Pants-discovered file, those components MUST carry
  `waybill:pants-resolve = <name>` matching the m223 convention
  (name derived from Pants map key or file stem, per m673 rules).

## Behavioral contract

### C1 — Dispatch triggered iff PEX parse fails (FR-002)

`pants/mod.rs::read` MUST invoke `uv::lockfile::parse(bytes)` iff:

1. The file was discovered by the m673 Pants pipeline (any of the 5 sources).
2. `pants::lockfile::parse(bytes)` returned `None` (parse failed).
3. The dispatch is a SECOND parse attempt on the SAME bytes.

Successful `pants::lockfile::parse` (PEX shape) MUST NOT trigger the
uv fallback — a file is EITHER PEX or UV, not both.

### C2 — Bytes shared, not re-read (Principle IV)

The `std::fs::read` result is already in memory when
`pants::lockfile::parse` runs. The uv fallback MUST NOT re-read the
file from disk — it reuses the existing `bytes: &[u8]` reference.
Prevents subtle race conditions (file mutated between reads) and
saves one syscall per fallback.

### C3 — WARN suppression on successful fallback

When `pants::lockfile::parse` returns `None`, it emits a WARN
(inherited m223 behavior). If `uv::lockfile::parse` then succeeds
on the same bytes, the earlier PEX WARN was WRONG — we actually
found a valid uv-shape lockfile. The reader MUST emit an INFO
(not WARN) log line saying "recognized as uv.lock format" so the
operator sees the resolution:

```
INFO uv-lock reader: recognized `<path>` as uv.lock format after Pex parse rejection; parsed <N> packages
```

**Note**: this does NOT retract the earlier PEX WARN — that log
line is already emitted and grep-searchable. The INFO complement
provides context. Alternative "swallow the PEX WARN on successful
uv fallback" was rejected because it adds branching complexity
inside `pants::lockfile::parse` for a rarely-hit case.

### C4 — Resolve-name propagation

When the uv fallback emits components, the emitted components MUST
carry `waybill:pants-resolve = <name>` where `<name>` is the m673
`DiscoveredLockfile.resolve_name` (either the m672 `[python.resolves]`
map key, or the m223 file-stem derivation). This matches m223 output
shape for Pants-tagged components.

Implementation: `uv::lockfile::to_entry` accepts an optional
`pants_resolve_name: Option<&str>` parameter. When `Some(name)`, the
emitted component carries the `waybill:pants-resolve` annotation.
When `None` (standalone uv.lock discovery), the annotation is
absent.

### C5 — Failure fallthrough

If BOTH `pants::lockfile::parse` AND `uv::lockfile::parse` return
`None`, the file is genuinely corrupt / unknown format. The uv
fallback MUST NOT emit any additional WARN — `pants::lockfile::parse`
already WARNed with its parse error, and `uv::lockfile::parse` will
have WARNed on its own parse failure. Two WARNs on the same file
is acceptable — the double-signal is informative (both formats
rejected).

### C6 — Standalone uv discovery unaffected

The Pants fallback is one of TWO discovery paths for the uv
reader. The other is `<scan_root>/uv.lock` at repo root (per FR-001).
When a repo has NO Pants signal (no `pants.toml`, no
`3rdparty/python/`, no `<root>/lockfiles/`) but DOES have
`<root>/uv.lock`, the standalone discovery path fires normally
without any Pants involvement.

Both discovery paths call the same `uv::lockfile::parse` — but with
different callers passing different `pants_resolve_name` values
(`None` for standalone, `Some(name)` for Pants fallback).

## Test matrix

| Repo shape | Expected uv reader activation | Expected component annotations |
|---|---|---|
| Only `<root>/uv.lock` | Standalone discovery, `parse` called once | No `waybill:pants-resolve` annotation |
| `pants.toml` `[python.resolves]` → uv-shape file at declared path | Pants FR-002 fallback, `parse` called after `pants::lockfile::parse` fails | `waybill:pants-resolve` = map key |
| `<root>/3rdparty/python/*.lock` file, uv-shape | m673 default-glob discovery → PEX fails → uv fallback | `waybill:pants-resolve` = file stem |
| `<root>/3rdparty/python/*.lock` file, PEX-shape | m223 PEX parse succeeds; uv NOT invoked | (m223 emission — unchanged) |
| `<root>/uv.lock` (standalone) AND `<root>/pants.toml` (empty) | Standalone uv discovery fires (uv.lock has priority); Pants reader emits zero-discovered hint | `waybill:python-lockfile-format=uv`; no pants-resolve |
| `<root>/3rdparty/python/foo.lock` (garbage — not PEX, not uv) | Both parsers WARN; no components emitted | (both WARN log lines present) |
| `<root>/uv.lock` (valid) AND `<root>/pants.toml` `[python.resolves]` uv-file | Both paths fire; canonical-path dedup collapses only if the SAME file is referenced | If distinct files: both emit; If same file via symlink: m672 map wins per m673 dedup |

## Interaction with m673 discovery pipeline

m673's `dedup_by_canonical_path` runs BEFORE the parse dispatch.
When the same file is discovered via multiple m673 sources (e.g.
`[python.resolves]` map + `<root>/*.lock` glob), the dedup collapses
to ONE `DiscoveredLockfile` with the m672 map-key precedence. That
single entry then goes through parse dispatch: try PEX first, fall
back to uv on failure.

The uv reader does NOT introduce a new discovery source — it's a
parser alternative for m673's existing sources.

## Non-goals

- **No new content-detection gate** for uv.lock at m673 discovery
  time. Reason: the m673 gate is FR-004 silent-skip for wide-scope
  paths (repo-root + lockfiles/). The Pants FR-002 fallback goes
  through `pants::lockfile::parse` first — which either succeeds
  (PEX shape) or WARNs (which the caller then treats as "try uv").
  Adding a pre-parse content-detect for uv would need a UV-signature
  helper (`is_uv_lockfile_content`) similar to `is_pex_lockfile_content`
  from m673. Deferred to v2 if empirically justified — for now, the
  try-PEX-then-uv sequence is cheap enough (both parsers early-exit
  on shape mismatch).
- **No standalone-uv discovery on Pants-shaped repos**. If a repo
  has a `<root>/uv.lock` (Astral's default) AND `<root>/pants.toml`
  (Pants signal), both paths fire independently — but they discover
  different files (uv.lock at root vs. Pants-declared paths under
  subdirectories). Canonical-path dedup at m673 handles same-file-via-
  multiple-paths.
