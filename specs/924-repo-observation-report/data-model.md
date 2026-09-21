# Phase 1 — Data Model: Repo Observation Report

**Feature**: 924-repo-observation-report · **Date**: 2026-09-21

Entities as they appear in the emitted document. Field names here are the
contract; see [contracts/report-schema.md](./contracts/report-schema.md) for
the serialised shape and versioning rules.

---

## ObservationReport (root)

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `{major: u16, minor: u16}` | FR-016 / FR-017a. Major bump = removal, rename, or changed meaning. |
| `schema_stability` | enum | Fixed at `alpha` for now (FR-017). |
| `tool_version` | string | Volatile — declared in `volatile_fields`. |
| `generated_at` | RFC 3339 | Volatile — declared in `volatile_fields`. |
| `volatile_fields` | list of field paths | FR-020. The exhaustive set a determinism comparison must mask. Self-describing so a differ needs no out-of-band knowledge. |
| `redaction_mode` | enum `none` \| `paths` | FR-019c. Always present, in both modes. |
| `significance_threshold` | u32 | FR-021c. The value that governed record-or-aggregate. |
| `totals` | `RepositoryTotals` | Repository-wide reconciliation. |
| `readers` | list of `ReaderCoverage` | Per-reader totals. |
| `directories` | list of `DirectoryObservation` | Significant directories only (FR-021a). |

**Validation**: `totals` MUST reconcile (FR-003). A report failing that is
invalid, not merely imperfect.

---

## RepositoryTotals

| Field | Type | Notes |
|---|---|---|
| `directories_walked` | u64 | All directories traversed, including aggregated ones. |
| `directories_recorded` | u64 | Length of `directories`. The FR-021/SC-008 bound applies here. |
| `files_walked` | u64 | |
| `files_claimed` | u64 | Claimed by ≥1 reader. A file claimed by several counts **once** (see edge case). |
| `files_unclaimed` | u64 | |
| `files_skipped` | map reason → u64 | Reasons enumerated, never aggregated into a single "other". |

**Invariant (FR-003)**: `files_walked == files_claimed + files_unclaimed + Σ files_skipped`.

---

## DirectoryObservation

One per **significant** directory (FR-021a): has a marker, was claimed, is a
scan/exclusion boundary, or is unclaimed and exceeds `significance_threshold`.

| Field | Type | Notes |
|---|---|---|
| `path` | string | Repository-relative. Never absolute (FR-019). Segment-hashed when `redaction_mode == paths` (FR-019b). |
| `claim_status` | enum | **Exclusive** (FR-012a): `claimed` \| `unclaimed` \| `excluded_by_policy`. |
| `claimed_by` | list of reader ids | Non-empty iff `claim_status == claimed`. |
| `ecosystems` | list of `EcosystemAttribution` | Zero or more. Independent of `claim_status` (FR-013). |
| `ambiguity` | `AmbiguityRecord?` | Optional, independent of `claim_status` (FR-012b). |
| `files_direct` | u64 | Files directly in this directory. |
| `files_aggregated` | u64 | Rolled up from descendants that earned no record (FR-021b). |
| `components_emitted` | u64 | FR-006. Zero here with a non-empty `claimed_by` is the FR-004 signal. |
| `observation` | `DirectoryObservationDetail?` | Present when not confidently classified (FR-011). |

**Why `claim_status` and `ambiguity` are separate fields**: a single verdict
cannot express "claimed **and** ambiguous", and that is the exact shape of
this repository's `waybill-cli/tests/` tree (SC-001). Collapsing them would
discard the signal the feature exists to surface.

**Why `files_direct` and `files_aggregated` are separate**: aggregation must
preserve counts for FR-003 without pretending the files are in this directory.
Summing them gives the subtree; reading `files_direct` alone gives the truth
about this directory.

---

## EcosystemAttribution

| Field | Type | Notes |
|---|---|---|
| `ecosystem` | string | e.g. `deno`, `julia`. |
| `evidence_marker` | string | The marker filename that produced the attribution. |
| `support` | enum | `supported` \| `no_reader`. |

**Validation (FR-008)**: an attribution MUST cite a marker file. Extensions
alone MUST NOT produce one — a directory's marker often sits above the source
it governs, so extension-derived attribution would mislabel exactly the
layouts this report exists to explain.

**Validation (R6)**: no table entry may name a marker that a registered reader
already claims. Enforced by test against the live registry, because a table
that silently rots is the failure mode this project has already experienced.

---

## AmbiguityRecord

| Field | Type | Notes |
|---|---|---|
| `kind` | enum | e.g. `multiple_ecosystem_lockfiles`, `unclassifiable_contents`. Extensible — FR-017c governs unknown members. |
| `interpretations` | list of string | Competing readings, e.g. polyglot project / test fixtures / vendored examples. **Never ranked.** |
| `evidence` | list of string | What was observed that produced the ambiguity (FR-015). |

**Validation (FR-014)**: `interpretations` MUST hold ≥2 entries. A single
interpretation is a classification, and belongs in `ecosystems` instead.

**Validation (FR-013)**: where several ecosystems are observed, all appear in
`ecosystems`; none is marked authoritative.

---

## DirectoryObservationDetail

Present when the directory is not confidently classified (FR-011).

| Field | Type | Notes |
|---|---|---|
| `file_count` | u64 | |
| `max_depth` | u32 | Below this directory. |
| `extension_histogram` | map ext → u64 | Observation only. MUST NOT drive attribution (FR-008). |
| `content_kind` | enum | `predominantly_binary` \| `predominantly_text` \| `mixed` \| `empty`. |
| `content_sample_bytes` | u32 | The per-file sample bound (8192 per R4). Present so a reader knows the verdict came from a sample. |

**Why `content_kind` earns its place**: 47 binary files and 47 text files are
both unclassified and mean entirely different things. This field is the
difference between a report that says "unknown" and one that is actionable.

---

## ReaderCoverage

| Field | Type | Notes |
|---|---|---|
| `reader_id` | string | |
| `files_matched` | u64 | |
| `components_emitted` | u64 | |

**Why both counts (FR-004)**: `files_matched > 0` with
`components_emitted == 0` is a reader that engaged and produced nothing — a
parse failure or an unsupported dialect. `files_matched == 0` is a reader that
never saw a candidate. Different diagnoses, different fixes; a single number
conflates them.

---

## Relationships

```
ObservationReport 1───* DirectoryObservation
                  1───* ReaderCoverage
                  1───1 RepositoryTotals

DirectoryObservation 1───* EcosystemAttribution      (independent of claim_status)
                     1───? AmbiguityRecord           (independent of claim_status)
                     1───? DirectoryObservationDetail (when not confidently classified)
```

## State transitions

None. The report is a single immutable observation of one traversal. It is
never updated in place, and nothing in it has a lifecycle — which is why
determinism (FR-020) is a comparison property rather than a concurrency one.
