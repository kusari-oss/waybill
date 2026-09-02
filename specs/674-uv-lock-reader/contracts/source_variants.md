# Contract — `UvSource` variants → PURL construction

**Feature**: 674-uv-lock-reader
**Applies to**: `waybill-cli/src/scan_fs/package_db/uv/source_variant.rs` + `uv/lockfile.rs::to_entry`

## Purpose

Define the 6-variant `UvSource` discriminator + the deterministic
per-variant mapping to PURL shapes, hash extraction, and per-
component annotations. Every uv.lock `[[package]]` entry routes
through exactly one variant.

## Variant enumeration

```rust
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum UvSource {
    Registry { registry: String },
    Git { git: String, rev: String },
    Path { path: String },
    Url { url: String },
    Editable { editable: String },
    Virtual { virtual: String },
}
```

## Per-variant emission rules

### C1 — `Registry { registry }` → `pkg:pypi/*` (FR-004)

Input:
```toml
source = { registry = "https://pypi.org/simple" }
```

Output PURL: `pkg:pypi/<normalized-name>@<version>` where
`normalized-name` is `pip::normalize_pypi_name_for_purl(name)`
(lowercase + `_` → `-`, dot preserved — matches pip + m223).

Additional annotations:
- **If** `registry != "https://pypi.org/simple"` (private registry):
  emit `waybill:pypi-source-url = <registry>` for provenance.
- **Else**: no extra annotation (default PyPI is implicit).

### C2 — `Git { git, rev }` → `pkg:generic/*` (FR-005)

Input:
```toml
source = { git = "https://example.com/repo.git", rev = "abc123def456" }
```

Output PURL: `pkg:generic/<name>@<version>` (name + version verbatim
from `[[package]]`; NO pypi-name-normalization applied because
`pkg:generic` doesn't have PyPI's normalization rules).

Required annotations:
- `waybill:source-type = "git"`
- `waybill:source-url = "<git-url>@<rev>"`
  (concatenated form matches m223's `pants/lockfile.rs::locked_req_to_entry`
  emission for git sources — same annotation values.)

### C3 — `Path { path }` → `pkg:generic/*` (FR-007)

Input:
```toml
source = { path = "../local-package" }
# OR:
source = { path = "/abs/path/local-package" }
```

Output PURL: `pkg:generic/<name>@<version>`.

Required annotations:
- `waybill:source-type = "path"`
- `waybill:source-url = "file://<path>"`
  (Absolute path emitted as `file://<abs>`; relative path emitted as
  `file://<rel>` — preserves the operator's declared shape.)

### C4 — `Url { url }` → `pkg:generic/*` (FR-007)

Input:
```toml
source = { url = "https://example.com/wheel.whl" }
```

Output PURL: `pkg:generic/<name>@<version>`.

Required annotations:
- `waybill:source-type = "url"`
- `waybill:source-url = "<url>"`

### C5 — `Editable { editable }` → SKIP (FR-006)

Input:
```toml
source = { editable = "." }
```

The `to_entry` function MUST return `None`. This variant represents
the pyproject.toml's own package installed in editable mode
(`pip install -e .`). Emitting from uv.lock would create a duplicate
main-module component (m127 root selector + m670 main-module emission
already handle it).

### C6 — `Virtual { virtual }` → SKIP (FR-006)

Input:
```toml
source = { virtual = "workspace-root" }
```

The `to_entry` function MUST return `None`. Virtual pseudo-packages
represent workspace-level references that don't correspond to
installable code; skipping them keeps the emitted SBOM aligned with
what's materialized on disk.

### C7 — Shared: every non-SKIP variant emits with these fields (FR-008 through FR-011)

- **`sbom_tier = Some("lockfile")`** — m003 convention; feeds the
  m191 reconciler's higher-tier-wins policy.
- **`hashes = [<every SHA-256 from sdist + wheels[]>]`** (deduped
  by hex-value) — SHA-256 is the only algorithm uv.lock emits.
- **`waybill:python-lockfile-format = "uv"`** (C157) — FR-011
  format-provenance annotation.
- **`waybill:source-files = "<uv.lock relative path>"`** (FR-009) —
  round-trip round-tripability for audit.
- **`waybill:pants-resolve = <name>`** iff called via Pants FR-002
  fallback — the `to_entry` function takes an optional `pants_resolve_name`
  parameter that's `Some(name)` when invoked from `pants/mod.rs::read`.

## Test matrix

| Variant | Expected PURL | Expected annotations |
|---|---|---|
| Registry, `https://pypi.org/simple`, name=`waybill-fixture-x`, ver=`1.0` | `pkg:pypi/waybill-fixture-x@1.0` | None extra beyond shared C7 |
| Registry, custom `https://internal.pypi/simple`, name=`X` | `pkg:pypi/x@1.0` | + `waybill:pypi-source-url` |
| Git, `https://ex.com/r.git`, rev=`abc123` | `pkg:generic/x@1.0` | + `source-type=git` + `source-url=https://ex.com/r.git@abc123` |
| Path, `../local` | `pkg:generic/x@1.0` | + `source-type=path` + `source-url=file://../local` |
| Path, `/abs/local` | `pkg:generic/x@1.0` | + `source-type=path` + `source-url=file:///abs/local` |
| Url, `https://ex.com/w.whl` | `pkg:generic/x@1.0` | + `source-type=url` + `source-url=https://ex.com/w.whl` |
| Editable, `.` | (SKIP — `to_entry` returns None) | — |
| Virtual, `root` | (SKIP — `to_entry` returns None) | — |

## Cross-reader consistency check (FR-015)

For any Registry-variant package with `name` + `version` + `registry = "https://pypi.org/simple"`, the emitted PURL MUST be BYTE-IDENTICAL to what the pip reader emits for the same package if it appears in a `poetry.lock`, `Pipfile.lock`, or `requirements.txt` context. Enforced by:

- Shared `pip::normalize_pypi_name_for_purl` helper (single source of truth for name normalization).
- Shared `Purl::new()` constructor via `waybill_common` (single source of truth for PURL string escaping).
- Integration test cross-checking pip-emitted-PURL == uv-emitted-PURL for a fixture that has both a `poetry.lock` AND a `uv.lock` with the same package.

## Non-goals

- **No PEP 503 name-normalization diff-detection**. The pip reader already handles PEP 503 partially (per m670 memory); m674 uses the same helper, inheriting whatever policy pip settled on.
- **No PURL qualifier construction from wheel filenames**. `pkg:pypi/<name>@<version>?...` qualifiers (e.g. `?filename=...`) are not emitted in v1 — matches m223 Pants shape.
- **No re-signing / re-hashing**. Hashes are extracted verbatim from uv.lock; no independent verification against the URL.
