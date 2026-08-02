# Contract: Coursier lockfile parse + `pants.toml` discovery + `PackageDbEntry` emission

**Consumer surface**:
`waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs::read(scan_root: &Path) -> Vec<PackageDbEntry>`

**Called from**:
`waybill-cli/src/scan_fs/package_db/mod.rs::read_all()` dispatcher
(new call site alongside `pip::read`, `pants::read`, `maven::read`,
etc.).

Documents the exact wire-format expectations for both files consumed,
the shape of every `PackageDbEntry` emitted, and the fail-open
behavior boundaries.

---

## Input contract A: Pants-generated coursier lockfile

**Path discovery** (union of both):
1. Default glob: `<scan_root>/3rdparty/jvm/*.lock` (FR-001).
2. If `<scan_root>/pants.toml` exists AND parses AND contains a
   `[jvm.resolves]` table, ALSO include every `<name> = <path>`
   entry (path interpreted relative to `scan_root`) (FR-004).

**Required file header** (FR-011 discriminator):

The file MUST contain the literal substring
`# --- BEGIN PANTS LOCKFILE METADATA` on some line before the first
`[[entries]]` block. Missing → skip with INFO log (not WARN — this
is expected for standalone coursier lockfiles).

**Required metadata JSON** (within the header comment block):

```jsonc
{
  "version": 1,
  "generated_with_requirements": [ /* array of coord-strings; ignored */ ]
}
```

`version` MUST equal `1`. Other values → WARN + skip.

**Required TOML body shape** (after header strip):

```toml
[[entries]]
directDependencies = [ /* array of coord-strings; may be empty */ ]
dependencies = [ /* array of coord-strings; may be empty */ ]
file_name = "..."  # optional

[entries.coord]
group = "<string>"
artifact = "<string>"
version = "<string>"
packaging = "<string>"  # optional; default "jar"
classifier = "<string>"  # optional
url = "<string>"  # optional

[entries.file_digest]
fingerprint = "<hex string>"
serialized_bytes_length = <u64>  # optional
```

Each `[[entries]]` is a distinct locked distribution. May be empty
(zero entries) — that's an INFO (partially-generated lockfile), not
a WARN.

**Coordinate-string shape** (per R2):

```text
coord_string = coord_triple ("," metadata_kv ("," metadata_kv)*)?
coord_triple = <group> ":" <artifact> ":" <version>
metadata_kv  = <key> "=" <value>
```

Waybill parses via `split_once(',')` + `splitn(3, ':')` — see
research.md §R2 for the reference implementation + edge-case table.

## Input contract B: `pants.toml` (optional)

```toml
[jvm]
default_resolve = "prod"       # optional; defaults to using filename stem

[jvm.resolves]                  # optional; empty when absent
prod = "build-support/jvm/prod.lock"
junit = "3rdparty/jvm/junit.lock"
# ... more entries
```

Any other keys ignored. If `[jvm]` absent, if `[jvm.resolves]`
absent, or if the config file is unparseable → fall back to default
glob per FR-004 (no failure).

---

## Fail-open behavior boundaries (FR-006 / FR-011 / SC-005)

Per-file diagnostics never abort the whole scan:

| Condition | Diagnostic level | Reader behavior |
|-----------|------------------|-----------------|
| Lockfile is missing the Pants metadata header comment | INFO | Skip this lockfile; process other candidates (FR-011) |
| Lockfile has header but metadata JSON is unparseable | WARN | Skip this lockfile |
| `metadata.version != 1` | WARN | Skip this lockfile |
| Lockfile TOML body is not valid TOML | WARN | Skip this lockfile |
| `entries` is missing or wrong type | WARN | Skip this lockfile |
| `entries` is `[]` (empty) | INFO | Emit zero components from this lockfile; success |
| `Entry.coord.group` / `.artifact` / `.version` missing or empty | WARN | Skip THIS entry (not the whole lockfile); continue |
| `Entry.file_digest` absent | INFO | Emit entry with empty `hashes[]` |
| `EntryFileDigest.fingerprint` is invalid hex | WARN | Emit entry with empty `hashes[]` |
| Coordinate-string parse failure (missing colons, empty segments) | WARN | Skip THIS edge (not the whole entry); continue with remaining edges |
| PURL construction failure (encoding error) | WARN | Skip THIS entry; continue |
| `pants.toml` present but not valid TOML | WARN | Fall back to default glob; do NOT abort |
| `pants.toml` `[jvm.resolves]` path doesn't exist on disk | WARN | Skip THAT resolve; process remaining candidates |

The reader ONLY returns `Vec<PackageDbEntry>`. It has no `Result`
return — errors are logged and swallowed per the fail-open contract.

---

## Output contract: `PackageDbEntry` emission

For each `Entry` that passes all validation gates above, one
`PackageDbEntry` is emitted. Fields marked `FIXED` have the same
value for every entry from this reader; fields marked `PER-ENTRY`
derive from the lockfile data.

| Field | Value / source | Type |
|-------|----------------|------|
| `purl` | PER-ENTRY: `pkg:maven/<group>/<artifact>@<version>` with `?classifier=<c>&type=<packaging>` qualifiers when applicable | `Purl` |
| `name` | PER-ENTRY: `EntryCoord.artifact` verbatim | `String` |
| `version` | PER-ENTRY: `EntryCoord.version` verbatim | `String` |
| `source_path` | PER-ENTRY: absolute path to the source lockfile | `PathBuf` |
| `depends` | PER-ENTRY: `Entry.dependencies[]` parsed via R2 → `<group>:<artifact>:<version>` strings | `Vec<String>` |
| `lifecycle_scope` | PER-ENTRY: JVM-dev-tool allowlist lookup (`scalatest`, `junit`, `mockito`, `scalafmt`, `ktlint`, `detekt`, plus generics) → `Development`, else `Runtime` | `Option<LifecycleScope>` |
| `sbom_tier` | FIXED: `Some("source".to_string())` | `Option<String>` |
| `hashes` | PER-ENTRY: 1 `ContentHash::sha256` from `EntryFileDigest.fingerprint` (empty vec if absent) | `Vec<ContentHash>` |
| `licenses` | FIXED: `Vec::new()` (coursier format doesn't carry licenses) | `Vec<SpdxExpression>` |
| `requirement_ranges` | FIXED: `Vec::new()` (locks are pinned) | `Vec<String>` |
| `extra_annotations` | PER-ENTRY: see table below | `BTreeMap<String, Value>` |
| All other fields | `None` / default | Match m223 posture |

### `extra_annotations` per entry

**Always present**:

| Key | Value |
|-----|-------|
| `waybill:pants-resolve` | Resolve name — filename stem OR `[jvm.resolves]`-declared name (config wins). Reuses C143 (shipped in m223). |

**Present iff data available**:

| Key | Value | Condition |
|-----|-------|-----------|
| `waybill:source-url` | `EntryCoord.url` verbatim | Present iff non-null and non-empty. Reuses C144 (shipped in m223). |

**No new `waybill:*` annotation keys** in v1. `file_name` +
`serialized_bytes_length` from the Deserialize types are diagnostic-
only and NOT emitted per data-model.md §"Decision on waybill:file-name"
(rationale: preserves zero-new-parity-work invariant vs m223; can be
promoted later without schema break).

---

## FR-010 scan-end INFO log

Emitted exactly once at the end of `pants_jvm::read()`:

```text
INFO waybill::scan_fs::package_db::pants_jvm: pants-coursier-jvm reader complete
  lockfiles_discovered=<N>
  lockfiles_parsed_ok=<N>
  lockfiles_skipped_corrupt=<N>
  lockfiles_skipped_non_pants=<N>
  components_emitted=<total>
```

The `lockfiles_skipped_non_pants` field is NEW vs m223 (FR-011
discrimination). Other four fields match m223's shape byte-for-byte
for grep consistency.

If no `pants.toml` AND no `3rdparty/jvm/*.lock` files found: reader
returns early without emitting any log (byte-identity guarantee per
SC-003).

---

## Dedup contract (FR-005) — reader-to-reconciler boundary

The pants-coursier-jvm reader emits `PackageDbEntry` records with:
- `sbom_tier = Some("source")` — lockfile-derived (authoritative)
- `hashes.len() > 0` when `EntryFileDigest` present (typical case)
- `source_path` = the lockfile's absolute path

The m191 reconciler (`waybill-cli/src/resolve/reconciler.rs`) applies
PURL-level dedup at emit time using existing precedence rules:

1. Grouping key: `purl` normalized (m197 rules).
2. When two entries have the same PURL:
   - Hash-bearing entry wins over hashless.
   - Otherwise, `sbom_tier` precedence: `"source"` > `"design"`.
3. Losing entry's `source_path` is recorded via
   `waybill:source-files` on the winning entry.

The pants-coursier-jvm reader's lockfile entries carry hashes, so
they win over `pom.xml` entries (which don't) automatically per
rule (2) — no reader-side changes required.

**Validated in m223 US2**: the same reconciler behavior was exercised
for the pex+requirements.txt case. Zero new reconciler work.

---

## Non-goals for v1

- **Standalone coursier lockfile support** (produced by direct
  `coursier` CLI without Pants): deferred per FR-011 — the header
  discriminator excludes them. Follow-up spec when demand emerges.
- **`BUILD` file parsing** (`jvm_artifact(...)`, `scala_source(...)`):
  design-tier signal that duplicates the lockfile's content.
  Deferred.
- **Coursier lockfile v2 schema** (hypothetical future Pants format):
  handled reactively via the metadata `version` guard. When Pants
  ships v2, waybill adds a v2-branch parser.
- **Marker-aware entry deduplication**: coursier doesn't include
  Python-style markers or platform tags at the entry level (Pants
  handles platform selection via resolve config), so this concern
  doesn't apply.
