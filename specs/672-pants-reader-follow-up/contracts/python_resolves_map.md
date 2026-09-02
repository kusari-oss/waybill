# Contract — `pants.toml` `[python.resolves]` map override

**Feature**: 672-pants-reader-follow-up
**Applies to**: `waybill-cli/src/scan_fs/package_db/pants/config.rs` (deserialization) + `waybill-cli/src/scan_fs/package_db/pants/mod.rs::discover_lockfiles` (walking the map into the discovery set).

## Purpose

Extend the Pants config parser to recognize the Pants 2.x
`[python.resolves]` TOML map (a map of `<resolve-name> → <path-string>`)
in addition to the pre-existing `[python].lockfile` singular field.
Every declared resolve whose path exists on disk contributes to the
lockfile discovery set alongside the default `3rdparty/python/*.lock`
glob.

## TOML shape recognized (v1 scope)

**In-scope** (Q2: bare-string only):

```toml
[python.resolves]
mypy = "3rdparty/python/mypy.lock"
altana-nexus = "3rdparty/python/altana-nexus.lock"
user_reqs = "build-support/py/user_reqs.lock"
```

**Out of scope** (v2 extension point):

```toml
# Table shape — WARNs and skips per FR-007
[python.resolves.altana-nexus]
path = "3rdparty/python/altana-nexus.lock"
lockfile-generator = "pex"

# Array or other non-string value — WARNs and skips per FR-007
[python.resolves]
oddball = ["not", "a", "string"]
```

## Behavioral contract

### C1 — Bare-string values populate discovery (FR-005)

For each `<key, value>` pair in `[python.resolves]` where `value` is
a bare TOML string, the reader MUST add
`(scan_root.join(value), /* resolve_name */ key)` to the candidate
lockfile set.

### C2 — Non-existent paths WARN + skip (FR-008)

If a bare-string value names a path that does NOT exist on disk after
`scan_root.join`, the reader MUST log a WARN naming both the resolve
name (map key) AND the missing path, and MUST NOT include the path in
`lockfiles_discovered`. Other entries in the same map remain honored.

**WARN message shape** (illustrative):
```
pants-pex reader: `[python.resolves]` entry `<key>` names a path that does not exist on disk; skipping. path=<missing_path>
```

### C3 — Non-bare-string values WARN + skip (FR-007 + clarify Q2)

If a value is NOT a bare TOML string (tables, arrays, integers,
booleans, floats, dates, inline `[python.resolves.<name>]` sections),
the reader MUST log a WARN naming the resolve name AND the observed
TOML type AND a migration hint, and MUST NOT include the entry in
`lockfiles_discovered`. Other bare-string entries in the same map
remain honored.

**WARN message shape** (illustrative):
```
pants-pex reader: `[python.resolves]` entry `<key>` has non-string value of TOML type `<type>`; skipping. m672 v1 supports bare-string values only. File a follow-up issue if table-shape parsing is needed. observed_shape=<value_debug>
```

### C4 — Dedup: map wins over default glob (FR-009)

When both the default `3rdparty/python/*.lock` glob AND
`[python.resolves]` name paths that canonicalize to the same file,
the reader MUST parse the file exactly once, and the emitted
`resolve_name` MUST be the map key (NOT the file-stem-derived name).

Canonicalization uses `std::fs::canonicalize` (follows symlinks —
matches Pants's own resolution rules per research.md R4).

### C5 — Legacy `[python].lockfile` still honored (FR-006)

When `pants.toml` declares BOTH `[python].lockfile = "..."` (legacy
singular) AND `[python.resolves] {...}` (map), the reader MUST honor
both (superset union). The legacy field emits with a file-stem-derived
resolve name (matches m223 verbatim); the map entries emit with their
map-key resolve names.

### C6 — Missing / malformed `pants.toml` gracefully degrades

If `pants.toml` doesn't exist, isn't valid TOML, or lacks a `[python]`
section, the reader falls through to the default glob only (no WARN
about the config's absence — that's the normal "no override" state).
Matches m223 behavior at `pants/mod.rs:58-99`.

### C7 — Empty map

If `[python.resolves]` is declared but empty (`resolves = {}`), the
reader treats it identically to the no-map case — no WARN, no
additions to the discovery set.

### C8 — Duplicate keys within `[python.resolves]`

TOML spec forbids duplicate keys — the underlying `toml` crate returns
a parse error at `toml::from_str`. This case is handled by C6 (pants.toml
malformed → fall through). No m672-specific handling required.

## Test matrix

| Input shape | Expected outcome | Passes contract |
|---|---|---|
| `[python.resolves] mypy = "3rdparty/python/mypy.lock"` (path exists) | Added to discovery set with `resolve_name=mypy` | C1 |
| Same but path missing on disk | WARN naming `mypy` + missing path; entry skipped | C2 |
| `[python.resolves.altana-nexus] path = "..."` (table shape) | WARN naming `altana-nexus` + `table`; entry skipped | C3 |
| `[python.resolves] weird = 42` (int value) | WARN naming `weird` + `integer`; entry skipped | C3 |
| Map's `mypy = "3rdparty/python/mypy.lock"` collides with `3rdparty/python/mypy.lock` default-glob match | Single parse; `resolve_name=mypy` (map wins) | C4 |
| Both `[python].lockfile = "legacy.lock"` AND `[python.resolves]` present | Union; legacy path emits with file-stem-derived name; map paths emit with map-key names | C5 |
| No `pants.toml` on disk | Default glob only; no WARN | C6 |
| `pants.toml` present but no `[python]` section | Default glob only; no WARN | C6 |
| `[python.resolves]` present but empty | Default glob only; no WARN | C7 |
| `pants.toml` with duplicate keys inside `[python.resolves]` | TOML parser error → whole pants.toml parse WARN + fall-through to default glob | C6 (m223 behavior; not m672's concern) |

## Failure modes

- **`[python.resolves]` parse fails** (TOML shape drift beyond bare-string / table / array — e.g. syntactically invalid TOML): `toml::from_str` fails on the WHOLE `pants.toml`. Reader falls through to default glob (matches m223 behavior at line 86–90 today).
- **Per-entry non-string value**: C3 handles — one WARN per offending entry, other entries still honored.
- **Per-entry missing path**: C2 handles — one WARN per missing path, others still honored.

## Performance envelope

Deserialization uses `BTreeMap<String, toml::Value>` — `toml::Value` is
a small tagged union. The map traversal is O(entries); each entry
does at most one `std::fs::canonicalize` (which is one `stat` syscall
in the happy case). For a real-world 24-resolve Pants monorepo, the
whole override path adds < 10 ms to scan startup — well within
SC-007's 5 ms budget for the stripper (which is a separate budget).

## Non-goals

- **No table-shape parsing** (`[python.resolves.<name>] path = "..."`). Deferred to v2; WARN-and-skip in v1.
- **No enumeration of `lockfile-generator`, `constraints_file`, or other Pants-specific fields** even if they appear in a table entry — waybill only cares about the `.lock` file's contents.
- **No propagation of the map to downstream components' annotations**. The `resolve_name` propagates via the existing m223 mechanism (per-component annotation carrying the resolve name); m672 doesn't add new annotation fields.
