# Contract: Pex lockfile parse + `pants.toml` discovery + `PackageDbEntry` emission

**Consumer surface**:
`waybill-cli/src/scan_fs/package_db/pants/mod.rs::read(scan_root: &Path) -> Vec<PackageDbEntry>`

**Called from**:
`waybill-cli/src/scan_fs/package_db/mod.rs::read_all()` dispatcher
(new call site alongside existing per-ecosystem reader dispatch).

Documents the exact wire-format expectations for both files
consumed + the exact shape of every `PackageDbEntry` emitted, and
the fail-open behavior boundaries.

---

## Input contract A: Pex lockfile JSON

**Path discovery** (union of both):
1. Default glob: `<scan_root>/3rdparty/python/*.lock` (FR-001).
2. If `<scan_root>/pants.toml` exists AND parses AND contains a
   `[python].lockfile = "..."` string value, ALSO include that path
   (interpreted relative to `scan_root`) (FR-004).

**Required top-level shape**:

```jsonc
{
  "pex_version": "<string matching /^2\\./>",
  "locked_resolves": [ /* array, may be empty */ ]
}
```

**Per-resolve required shape**:

```jsonc
{
  "locked_requirements": [ /* array, may be empty */ ]
}
```

**Per-locked-requirement required shape**:

```jsonc
{
  "project_name": "<non-empty string>",
  "version": "<non-empty string>",
  "artifacts": [ /* array; may be empty for local-path-only entries */ ],
  "requires_dists": [ /* array; may be empty */ ],
  "requires_python": "<string OR null>"
}
```

**Per-artifact required shape**:

```jsonc
{
  "algorithm": "sha256",
  "hash": "<hex string>",
  "url": "<URL string>"
}
```

## Input contract B: `pants.toml` (optional)

```toml
[python]
lockfile = "path/relative/to/scan-root.lock"
# Any other keys ignored. If `[python]` absent, if `lockfile` absent,
# or if the value is not a string → fall back to default glob per FR-004.
```

---

## Fail-open behavior boundaries (FR-006 / SC-005)

The reader NEVER aborts the scan on per-file corruption. The following
are per-file WARN diagnostics + skip:

| Condition | Diagnostic level | Reader behavior |
|-----------|------------------|-----------------|
| Lockfile is not valid JSON | WARN | Skip this lockfile; process other candidates |
| `pex_version` missing OR not matching `^2\.` | WARN | Skip this lockfile |
| `locked_resolves` is missing or wrong type | WARN | Skip this lockfile |
| `locked_resolves` is `[]` (empty) | INFO | Emit zero entries from this lockfile; success |
| `LockedRequirement.project_name` missing or empty | WARN | Skip THIS entry (not the whole lockfile); continue |
| `LockedRequirement.version` missing or empty | WARN | Skip THIS entry; continue |
| `LockedRequirement.artifacts` is `[]` | INFO | Emit entry with empty `hashes` vector |
| `Artifact.algorithm` != "sha256" | INFO | Emit hash with the recorded algorithm anyway (future-proof) |
| PURL construction fails (e.g., invalid name after normalization) | WARN | Skip THIS entry; continue |
| `pants.toml` present but not valid TOML | WARN | Fall back to default glob; do NOT abort |
| `pants.toml` `[python].lockfile` path doesn't exist on disk | WARN | Fall back to default glob; do NOT abort |

The reader ONLY returns `Vec<PackageDbEntry>`. It has no `Result`
return — errors are logged and swallowed per the fail-open contract.

---

## Output contract: `PackageDbEntry` emission

For each `LockedRequirement` that passes all validation gates above,
one `PackageDbEntry` is emitted with the fields below. Fields marked
`FIXED` have the same value for every entry from this reader; fields
marked `PER-ENTRY` derive from the lockfile data.

| Field | Value / source | Type |
|-------|----------------|------|
| `purl` | PER-ENTRY: `pkg:pypi/<normalized-name>@<version>` if `artifacts[0].url` starts with `https://files.pythonhosted.org/`, else `pkg:generic/<normalized-name>@<version>` | `Purl` |
| `name` | PER-ENTRY: normalized project_name (per R3: lowercase, `_`/`.` → `-`) | `String` |
| `version` | PER-ENTRY: `LockedRequirement.version` verbatim | `String` |
| `source_path` | PER-ENTRY: absolute path to the source lockfile | `PathBuf` |
| `depends` | PER-ENTRY: `requires_dists[]` with project names extracted from PEP 508 strings (strip version specifiers, extras, markers) | `Vec<String>` |
| `lifecycle_scope` | PER-ENTRY: `Dev` if lockfile filename stem is in R2 allowlist, else `Runtime` | `Option<LifecycleScope>` |
| `sbom_tier` | FIXED: `Some("source".to_string())` | `Option<String>` |
| `evidence_kind` | FIXED: `EvidenceKind::Lockfile` (existing variant) | `EvidenceKind` |
| `hashes` | PER-ENTRY: one `ContentHash` per artifact | `Vec<ContentHash>` |
| `licenses` | FIXED: `Vec::new()` (Pex lockfile doesn't carry licenses) | `Vec<License>` |
| `requirement_ranges` | FIXED: `Vec::new()` (pinned versions only) | `Vec<RequirementRange>` |
| `extra_annotations` | PER-ENTRY: see table below | `BTreeMap<String, Value>` |

### `extra_annotations` per entry

**Always present**:

| Key | Value |
|-----|-------|
| `waybill:pants-resolve` | Resolve name — lockfile filename stem (e.g., `"default"`, `"mypy"`, `"pytest"`). For non-standard paths from `pants.toml`, use the filename stem verbatim. |

**Present iff data available**:

| Key | Value | Condition |
|-----|-------|-----------|
| `waybill:requires-python` | `LockedRequirement.requires_python` verbatim | `requires_python` is a non-null non-empty string |

**Present iff PURL is `pkg:generic/*`** (non-PyPI source per Q2 A):

| Key | Value |
|-----|-------|
| `waybill:source-url` | `artifacts[0].url` verbatim (or the scan-root-relative path for `file://` / absolute local paths per FR-009 privacy rule) |
| `waybill:source-type` | One of: `"git"`, `"url"`, `"local"` |

---

## FR-010 scan-end INFO log

Emitted exactly once at the end of `pants::read()`:

```text
INFO waybill::scan_fs::package_db::pants: pants-pex reader complete
  lockfiles_discovered=<N>
  lockfiles_parsed_ok=<N-corrupt>
  lockfiles_skipped_corrupt=<corrupt>
  components_emitted=<total>
```

Structured fields via `tracing`'s field syntax — SREs can filter logs
by any of the four counters. Zero-value fields are still emitted (for
consistent grep-ability).

If no `pants.toml` AND no `3rdparty/python/*.lock` files found: reader
returns early without emitting any log (byte-identity guarantee per
SC-003).

---

## Dedup contract (FR-005) — reader-to-reconciler boundary

The pants-pex reader emits `PackageDbEntry` records with:
- `sbom_tier = Some("source")` — lockfile-derived (authoritative)
- `evidence_kind = EvidenceKind::Lockfile`
- `source_path` = the lockfile's absolute path

The m191 reconciler (`waybill-cli/src/resolve/reconciler.rs`) applies
PURL-level dedup at emit time using existing precedence rules:

1. Grouping key: `purl` normalized (m197 rules).
2. When two entries have the same PURL:
   - If one has `hashes.len() > 0` and the other doesn't, the
     hash-bearing entry wins.
   - Otherwise, `sbom_tier` precedence: `"source"` > `"design"` (per
     existing reconciler rule; installed-DB `"deployed"` is
     independent and shouldn't collide with source-tier).
3. Losing entry's `source_path` is recorded via
   `waybill:also-detected-via` on the winning entry.

The pants-pex reader's lockfile entries carry hashes, so they win
over `requirements.txt` entries (which don't) automatically per
rule (2) — no reader-side changes required.

---

## Non-goals for v1

- **Coursier lockfile support (Pants JVM)**: separate follow-up spec.
  This reader is Python-only.
- **BUILD file parsing**: separate follow-up. Design-tier signal from
  BUILD files is marginal when the lockfile is authoritative.
- **Pex 1.x plaintext lockfile format**: out of scope. Pants 2.x uses
  Pex 2.x exclusively.
- **Marker-aware entry deduplication**: `LockedResolve.marker` +
  `platform_tag` are ignored; the first-encountered entry wins if
  multiple resolves record the same `project_name`. Multi-platform
  refinement deferred.
- **Registry authentication for lockfile URLs**: waybill parses URLs
  but never fetches from them. Registry auth is a Fulcio/Rekor-scope
  concern, not this reader's.
