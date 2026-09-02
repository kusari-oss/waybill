# Contract — uv.lock v1 schema

**Feature**: 674-uv-lock-reader
**Applies to**: `waybill-cli/src/scan_fs/package_db/uv/lockfile.rs`

## Purpose

Define the strict TOML schema that `uv::lockfile::parse` accepts.
Documents every required + optional field, every deserialization
behavior on shape drift, and the version-gate.

## Top-level document schema

```toml
version = <integer>                          # REQUIRED — must equal 1 for m674 v1
revision = <integer>                         # OPTIONAL — Astral's internal patch counter
requires-python = "<PEP 440 constraint>"     # OPTIONAL — e.g. ">=3.10"
resolution-markers = [<marker>, ...]         # OPTIONAL — array of marker strings
supported-markers = [<marker>, ...]          # OPTIONAL

[options]
# freeform table — reader silently ignores every field inside

[manifest]
# freeform table — reader silently ignores every field inside

[[package]]
# array of tables — one entry per locked package (see § Package schema)
```

## Package schema (`[[package]]`)

```toml
[[package]]
name = "<PyPI-name>"                         # REQUIRED — string
version = "<PEP 440 version>"                # REQUIRED — string
source = { <variant-discriminator> = "..." } # REQUIRED — inline table (see § Source variants)

# Optional fields:
dependencies = [                             # OPTIONAL — array of inline tables
    { name = "<dep-name>", extra = [...] },  #    each dep has REQUIRED name + OPTIONAL extra + marker
    ...
]
sdist = { url = "...", hash = "sha256:...", size = <integer> }   # OPTIONAL — inline table
wheels = [                                                        # OPTIONAL — array of inline tables
    { url = "...", hash = "sha256:...", size = <integer> },       #    same shape as sdist
    ...
]
# Unknown fields: silently ignored (matches m223 format-evolution tolerance).
```

## Source variants — `source = { ... }`

Exactly ONE of the following discriminator keys MUST be present in the inline table:

| Discriminator key | Companion required keys | Meaning |
|---|---|---|
| `registry` | (none) | Package resolved from a Python package registry (usually `https://pypi.org/simple`). |
| `git` | `rev` (required) | Package cloned from a Git repository at a specific revision. |
| `path` | (none) | Package installed from a local filesystem path. |
| `url` | (none) | Package installed from a direct HTTP(S) URL. |
| `editable` | (none) | The pyproject.toml's own package installed in editable mode. |
| `virtual` | (none) | A virtual pseudo-package (no installable code — e.g. a workspace root). |

**Serde dispatch strategy**: `#[serde(untagged)]` on the `UvSource` enum tries variants in declaration order and picks the first that deserializes successfully. Because each variant's field name is unique, the untagged strategy has no ambiguity.

## Behavioral contract

### C1 — Version-gate (FR-003)

`uv::lockfile::parse(bytes)` MUST return `None` and emit a WARN log iff the top-level `version` field is anything other than the integer `1`. Log line: `uv-lock reader: unsupported uv.lock schema version=<N>; skipping`.

### C2 — Strict deserialization (Principle IV)

The reader MUST use `toml::from_slice` with the strict `UvLockfile` / `UvPackage` structs (NOT `toml::Value` permissive parsing). Shape drift at either level (missing required field, wrong type, unknown variant discriminator, etc.) MUST cause the whole file's parse to fail. Failure returns `None` + emits a WARN log naming the parse error.

### C3 — Unknown-field tolerance

The reader MUST NOT set `#[serde(deny_unknown_fields)]` on any of `UvLockfile`, `UvPackage`, `UvDependency`, or `UvHashArtifact`. Unknown fields are silently ignored so future-version uv.lock schemas that add fields without changing the top-level `version` bump remain parseable. This matches m223 Pex format-evolution posture.

### C4 — Empty lockfile

A uv.lock with `version = 1` and NO `[[package]]` entries MUST parse successfully and return `Some(UvLockfile { packages: Vec::new(), ... })`. Downstream emission produces 0 components — no error, no WARN.

### C5 — Byte-identity on non-uv repos

`uv::lockfile::parse` MUST NOT be invoked on scans of repos that have no `<scan_root>/uv.lock` AND no Pants FR-002 fallback trigger. Confirmed by the `uv::read` orchestrator's early-return on missing discovery target (see FR-013).

### C6 — Fail-open contract inherited from m223

Every WARN + skip path (version mismatch, parse failure) MUST leave the scan running. A malformed uv.lock file MUST NOT abort the entire scan.

## Illustrative examples

### Accept: minimal valid uv.lock (FR-001 happy path)

```toml
version = 1
requires-python = ">=3.10"

[[package]]
name = "waybill-fixture-alpha"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
```

→ `Some(UvLockfile { version: 1, packages: [ 1 UvPackage ], ... })`; emits 1 pypi component.

### Accept: multi-source uv.lock (§ Variants)

```toml
version = 1

[[package]]
name = "waybill-fixture-git"
version = "0.5.0"
source = { git = "https://example.com/repo.git", rev = "abc123" }

[[package]]
name = "waybill-fixture-registry"
version = "2.0.0"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "waybill-fixture-editable"
version = "0.1.0"
source = { editable = "." }
```

→ Parses successfully; emits 2 components (git → pkg:generic + registry → pkg:pypi; editable is SKIP per FR-006).

### Reject: version drift (C1)

```toml
version = 2
[[package]]
name = "x"
version = "1.0"
source = { registry = "https://pypi.org/simple" }
```

→ Reader returns `None`; WARN: `uv-lock reader: unsupported uv.lock schema version=2; skipping`.

### Reject: missing required field (C2)

```toml
version = 1
[[package]]
name = "waybill-fixture-x"
# missing `version` field
source = { registry = "https://pypi.org/simple" }
```

→ Reader returns `None`; WARN naming the parse error at line pointing to the malformed `[[package]]`.

### Reject: unknown source variant (C2)

```toml
version = 1
[[package]]
name = "x"
version = "1.0"
source = { fancynewvariant = "..." }
```

→ Reader returns `None` (no `UvSource` variant matches); WARN naming the failed serde-untagged dispatch.

### Ignore silently: unknown top-level field (C3)

```toml
version = 1
future-astral-flag = true
[[package]]
name = "x"
version = "1.0"
source = { registry = "https://pypi.org/simple" }
```

→ Parses; emits 1 component. `future-astral-flag` silently ignored.

## Test matrix

| Fixture | Expected outcome | Passes contract |
|---|---|---|
| minimal 1-registry-package uv.lock | 1 pypi component | C4-accept + FR-004 |
| multi-source 6-variant uv.lock | 4 emitted (registry + git + path + url; editable + virtual SKIP) | C2 + FR-004..FR-007 + FR-006 |
| version=2 uv.lock | 0 components + version-gate WARN | C1 |
| empty `[[package]]` uv.lock | 0 components + INFO log | C4 |
| malformed TOML | 0 components + parse-error WARN | C2 |
| unknown top-level field | Parses normally; ignored | C3 |
| Multi-platform wheels (10 wheels, 3 unique hashes) | 1 component with 3 SHA-256 hashes attached | FR-008 dedup |
