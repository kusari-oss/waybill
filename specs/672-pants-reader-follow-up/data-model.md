# Phase 1 Data Model — m672 Pants pex-lockfile reader follow-up

**Date**: 2026-09-01

## Overview

Three concrete data-shape changes inside `waybill-cli/src/scan_fs/package_db/pants/`:

1. `PythonSection` extends with a `resolves: BTreeMap<String, toml::Value>` field.
2. `DiscoveredLockfile` extends with an `origin: DiscoverySource` field to trace how each candidate got discovered (needed for FR-009 map-wins-on-dedup logic).
3. A new `LegacyShapeCounter` struct threads through the parse loop to feed the FR-013 log field.

No SBOM wire-format changes. No parity-catalog changes. All state is per-scan in-process (matches every reader milestone since m002).

---

## Struct 1: `PythonSection` (extended)

**File**: `waybill-cli/src/scan_fs/package_db/pants/config.rs`

**Pre-m672 shape**:
```rust
#[derive(Deserialize)]
pub(crate) struct PythonSection {
    #[serde(default)]
    pub(crate) lockfile: Option<String>,
}
```

**Post-m672 shape**:
```rust
#[derive(Deserialize)]
pub(crate) struct PythonSection {
    /// Legacy singular-lockfile field (pre-Pants-2.x). Preserved for
    /// backward compatibility with m223. When present, waybill unions
    /// this path with the resolves map and the default glob.
    #[serde(default)]
    pub(crate) lockfile: Option<String>,

    /// Pants 2.x `[python.resolves]` map. Keys are operator-supplied
    /// resolve names (e.g. `mypy`, `internal-libs`); values are the
    /// filesystem path to the resolve's lockfile (bare TOML string).
    /// Non-bare-string values are deserialized as `toml::Value` here
    /// so we can name the offending type in the WARN log, then
    /// WARN-and-skip at the caller (see FR-007).
    #[serde(default)]
    pub(crate) resolves: BTreeMap<String, toml::Value>,
}
```

**Validation rules**:
- `resolves` deserialization NEVER fails on shape drift — `toml::Value` accepts any TOML value shape.
- Empty map (`resolves = {}` or absent) is valid (equivalent to the pre-m672 no-op case).
- Duplicate keys within `[python.resolves]` are handled by TOML spec (last-write-wins at the TOML parser level; not m672's concern).

**Field ordering**: `#[derive(Deserialize)]` field order in the struct is stable in memory but not observable in JSON/TOML output (this is deserialize-only — no `Serialize` derive). Adding `resolves` after `lockfile` is a source-code convenience, not a wire-format constraint.

---

## Struct 2: `DiscoveredLockfile` (extended)

**File**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs`

**Pre-m672 shape**:
```rust
struct DiscoveredLockfile {
    path: PathBuf,
    resolve_name: String,
}
```

**Post-m672 shape**:
```rust
struct DiscoveredLockfile {
    /// Canonicalized (via `std::fs::canonicalize`) absolute path. All
    /// downstream comparison + dedup operates on this canonical form
    /// per FR-009.
    path: PathBuf,

    /// Resolve name — either the file-stem-derived name (default glob
    /// origin) or the pants.toml map key (resolves-map origin). See
    /// `origin` field below for the discriminator.
    resolve_name: String,

    /// How this lockfile got discovered. Drives FR-009's map-wins-on-
    /// dedup: when two `DiscoveredLockfile`s share the same canonical
    /// `path`, the one with `origin == PythonResolvesMap` REPLACES
    /// any `origin == DefaultGlob` sibling (map key is authoritative).
    origin: DiscoverySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoverySource {
    /// Found via the `3rdparty/python/*.lock` default glob.
    /// Resolve name derived from `path.file_stem()`.
    DefaultGlob,
    /// Found via `pants.toml` `[python].lockfile` singular key
    /// (legacy Pants shape). Resolve name derived from
    /// `path.file_stem()` (matches m223 behavior).
    PythonLockfileSingular,
    /// Found via `pants.toml` `[python.resolves]` map. Resolve name
    /// is the map's KEY (authoritative over file-stem derivation).
    PythonResolvesMap,
}
```

**Dedup relation** (per FR-009):

Two `DiscoveredLockfile`s A and B are considered the same iff `A.path == B.path` after `std::fs::canonicalize`. When dedup fires:

- If exactly one has `origin == PythonResolvesMap`, keep that one.
- Else, keep the one with the LEXICALLY FIRST `resolve_name` (deterministic tie-breaker for the rare `DefaultGlob` × `PythonLockfileSingular` collision).

**State transitions**: Discovery is a single-pass builder. No mutations after the discovery-loop return.

---

## Struct 3: `LegacyShapeCounter` (new)

**File**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs`

```rust
/// Milestone 672 FR-013: count how many lockfiles in this scan
/// carried the pre-Pants-2.30 `//`-comment metadata block (i.e., the
/// prefix stripper actually consumed at least one `//` line before
/// handing bytes to the JSON parser). Log-line only in v1; a v2
/// milestone may promote this to a document-scope annotation.
#[derive(Debug, Default)]
struct LegacyShapeCounter {
    count: usize,
}

impl LegacyShapeCounter {
    fn record_stripped(&mut self, stripped_bytes: usize) {
        // The stripper returns the (potentially shortened) body
        // slice; the caller passes `original.len() - body.len()` here.
        // A non-zero value means at least one `//` line was consumed.
        if stripped_bytes > 0 {
            self.count += 1;
        }
    }

    fn as_log_value(&self) -> usize {
        self.count
    }
}
```

**Validation rules**:
- Counter is monotonically non-decreasing during a scan.
- Emission is unconditional at the reader-complete log line (per FR-013 — value 0 is fine to emit; the JSON envelope is byte-identical).

**Alternative considered**: threading a `Vec<PathBuf>` of the actual legacy-shape file paths for DEBUG-level per-file logging. Deferred to v2 per R5.

---

## Struct 4: Front-matter stripper — pure function, no persistent state

**File**: `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs`

```rust
/// Milestone 672 FR-001/FR-002/FR-003: strip a leading `//`-comment
/// metadata block (Pants ≤ 2.29 lockfile shape) from `bytes`. Returns
/// the slice starting at the first non-`//` non-whitespace line, or
/// `&[]` if the entire input was `//`-commented.
///
/// This is a pure function — no allocation, no error path, no
/// persistent state. Callers pass its output directly to
/// `serde_json::from_slice`.
///
/// Complexity: O(prefix-length) — the loop bails out at the first
/// non-`//` line. On clean-JSON input (first non-whitespace byte is
/// `{`), the function returns after examining a single line.
fn strip_pants_frontmatter(bytes: &[u8]) -> &[u8] {
    // Implementation per research.md R2.
}
```

**Contract**:
- Input: raw file bytes as read by `std::fs::read`.
- Output: a subslice of the input starting at the JSON body's first byte (or empty if no non-`//` line was found).
- Failure modes: none — a fully-commented file returns empty, which downstream `serde_json::from_slice` fails on with the standard WARN + skip.

---

## Data-flow diagram

```
              ┌─────────────────────────┐
              │ pants.toml (if exists)  │
              └────────────┬────────────┘
                           │ toml::from_str
                           ▼
              ┌─────────────────────────┐
              │ PythonSection {         │
              │   lockfile: Option<..>  │
              │   resolves: BTreeMap<>  │
              │ }                       │
              └────────────┬────────────┘
                           │
             ┌─────────────┼─────────────┐
             │             │             │
             ▼             ▼             ▼
      DefaultGlob    Singular      ResolvesMap
      (3rdparty/     (legacy       (map key
       python/*)     Pants ≤ 2.x)  authoritative)
             │             │             │
             │             │             │
             ▼             ▼             ▼
          canonicalize + union + dedup (FR-009)
                           │
                           ▼
              ┌─────────────────────────┐
              │ Vec<DiscoveredLockfile> │
              └────────────┬────────────┘
                           │
                           ▼
              ┌─────────────────────────┐
              │ std::fs::read(path)     │
              │        → &[u8]          │
              └────────────┬────────────┘
                           │
                           ▼
              ┌─────────────────────────┐
              │ strip_pants_frontmatter │
              │  (always-strip, Q3)     │
              └────────────┬────────────┘
                           │ &[u8] (JSON body)
                           ▼
              ┌─────────────────────────┐
              │ serde_json::from_slice  │
              │      → PexLockfile      │
              │  (WARN + skip on fail)  │
              └────────────┬────────────┘
                           │
                           ▼
              (per-req) locked_req_to_entry
                           │
                           ▼
              ┌─────────────────────────┐
              │ Vec<PackageDbEntry>     │
              └─────────────────────────┘
```

---

## Non-goals

- No `Serialize` impls on any new struct — everything is deserialize-only or in-memory state.
- No `Clone` on `DiscoverySource` beyond `Copy` (it's a 3-variant enum; `Copy` suffices).
- No new public API — `DiscoveredLockfile` + `DiscoverySource` + `LegacyShapeCounter` are all `pub(super)` or module-private.
