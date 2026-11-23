# Data model: dynamic linkage and embedded source paths are labelled binary analysis

No new type is emitted into a document (FR-007).

## The technique decision (`scan_fs/binary/mod.rs`)

`binary_kind_technique(kind: &str) -> Option<ResolutionTechnique>`, defined over `BINARY_EVIDENCE_KINDS`:

| Kind | Result |
|---|---|
| `embedded-version-string`, `symbol-fingerprint`, `dynamic-linkage`, `embedded-source-path` | `Some(BinaryAnalysis)` |
| `cargo-auditable`, `elf-note-package`, `jdk-runtime-collapsed`, `python-stdlib-collapsed` | `Some(PackageDatabase)` |
| any other string, including every non-binary reader's kind | `None` |

Invariants:
- Every entry of `BINARY_EVIDENCE_KINDS` maps to `Some` (unit test).
- `BINARY_EVIDENCE_KINDS` is kept complete by its existing drift test.

## Uses

| Site | Before (spec 1646) | After |
|---|---|---|
| `scan_fs/mod.rs::entry_technique`, `nix/closure/contents.rs::component_from_entry` | `BinaryAnalysis` if the kind is in `deduplicator::BINARY_KINDS` | `binary_kind_technique(kind).unwrap_or(PackageDatabase)` |
| `deduplicator::effective_technique`, `is_binary_side` | the kind is in `BINARY_KINDS` | `binary_kind_technique(kind) == Some(BinaryAnalysis)` |
| `deduplicator::BINARY_KINDS` | two kinds | removed |

## Census events (DEBUG `waybill::version_census`, log-only commit unless stated)

| Event | When | Fields |
|---|---|---|
| `technique_change` | an entry's decided technique differs from the one it is converted with | `evidence_kind`, `purl`, `old`, `new` (spec 1646's shape) |
| `kind_merge` | a merge at S1 or S2 where either side's kind is `dynamic-linkage` or `embedded-source-path` | `site`, `purl`, and `winner` and `loser`, each `{evidence_kind, mechanism}` |
| `binary_derived_kind` | as in spec 1646, except that the `detected_go` value is renamed `detected-go` | `evidence_kind`, `purl`, `technique` |
| `binary_derived_kind` with `evidence_kind: "go-buildinfo"` | each entry `go_binary.rs::emit_entries_from_info` creates | `purl`, `path` (the binary), `technique` |

At the wired commit, `technique_change` and `kind_merge` are removed. The two `binary_derived_kind` corrections stay.
