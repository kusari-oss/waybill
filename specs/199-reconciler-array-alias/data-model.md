# Data Model: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Date**: 2026-07-15
**Purpose**: Shape of the 3 new/rotated annotations. All 3 documented in m197 data-model (E1/E2/E3); m199 restates concisely + notes what changes vs the m191-shipped shape.

## E1: `mikebom:declared-as` (NEW annotation)

**Location**: On any reconciler-survivor component whose reconciliation consumed one or more design-tier hits declared via an npm alias.

**Wire shape**: JSON array of strings — the ORIGINAL alias name(s) as written in source manifests.

**Example (single-declaration)**:
```json
{
  "extra_annotations": {
    "mikebom:declared-as": ["my-alias"]
  }
}
```

**Example (multi-manifest with different aliases)**:
```json
{
  "extra_annotations": {
    "mikebom:declared-as": ["another-alias", "my-alias"]
  }
}
```

**Validation rules** (inherited from m197 clarify + analyze):
- Array MUST be non-empty when the annotation is present.
- Values sorted lex-ascending for deterministic goldens.
- **Duplicates deduped** — the same alias declared in multiple manifests collapses to a single array entry (per m197 analyze-phase I1 remediation). Rationale: an alias is a name-mapping; count is not provenance.
- Values are raw alias strings from the manifest — NO npm-namespace prefixing, NO version suffix.
- Emitted ONLY when at least one alias was involved. Components without alias involvement have NO `mikebom:declared-as` key.

## E2: `mikebom:requirement-ranges` (ROTATED from m191 singular scalar)

**Location**: On every reconciler-survivor with at least one design-tier match.

**Wire shape**: JSON array of range strings.

**Example (single-declaration)**:
```json
{
  "extra_annotations": {
    "mikebom:requirement-ranges": ["^11.0"]
  }
}
```

**Example (multi-declaration)**:
```json
{
  "extra_annotations": {
    "mikebom:requirement-ranges": ["^11.0", "^11.1.0"]
  }
}
```

**Validation rules**:
- Array MUST be non-empty when present.
- Values are raw range strings from the manifest — NO normalization.
- **Duplicates PRESERVED** (unlike E1). Rationale: range/manifest count IS provenance signal ("3 manifests declared this range" is meaningful).
- Ordering: reordered 1:1 with `mikebom:source-manifests` (Nth range came from Nth manifest, where the manifest ordering is lex-ascending).

**Removed from m191**: the singular scalar `mikebom:requirement-range` key MUST NOT appear on m199-post reconciler survivors.

## E3: `mikebom:source-manifests` (ROTATED from m191 singular scalar)

**Location**: Same as E2 — every reconciler-survivor with at least one design-tier match.

**Wire shape**: JSON array of workspace-relative manifest paths.

**Example (single-declaration)**:
```json
{
  "extra_annotations": {
    "mikebom:source-manifests": ["packages/foo/package.json"]
  }
}
```

**Example (multi-declaration)**:
```json
{
  "extra_annotations": {
    "mikebom:source-manifests": [
      "packages/bar/package.json",
      "packages/foo/package.json"
    ]
  }
}
```

**Validation rules**:
- Array MUST be non-empty when present.
- Values are workspace-relative paths.
- Sorted lex-ascending (determinism gate for golden byte-identity).
- Ordering 1:1 with `mikebom:requirement-ranges` (E2).

**Removed from m191**: the singular scalar `mikebom:source-manifest` key MUST NOT appear on m199-post reconciler survivors.

## Reused Entity: `AliasResolution` (existing at `alias_mapping.rs:37`)

No shape change — the existing struct already has:
- `local_name: String` — the alias name (what appears as the dep key in package.json)
- `aliased_name: String` — the resolved package name (from `npm:<name>@<ver>`)
- `aliased_version: String` — the version-spec side of the alias
- `ecosystem: AliasEcosystem` — enum (`Npm` variant used by m199)

m199 introduces a new parser function `parse_package_json_alias()` that produces this struct from the `"my-alias": "npm:actual@ver"` package.json form. The pnpm-lock.yaml parser `detect_pnpm_alias` (m159) continues to produce the same struct from its own context — reconciler match logic doesn't distinguish source.

## Reconciler flow (post-m199)

```text
[Design-tier component from npm reader]
  ├── package.json entry parsed
  ├── parse_package_json_alias(dep_name, dep_ver_raw) called
  │      │
  │      ├── returns Some(AliasResolution{local, aliased, ...})
  │      │      │
  │      │      ├── PackageDbEntry created with PURL keyed on aliased_name
  │      │      └── extra_annotations["mikebom:declared-as"] = [local_name]
  │      │
  │      └── returns None (regular dep) → PURL keyed on dep_name, no declared-as annotation
  │
  └────────────────────────────────  Reconciler.reconcile_design_source_tiers()
                                                    │
                                                    ▼
              Match-key: (ecosystem, canonical_name_from_purl, source_manifest_dir)
              (canonical_name comes from the PURL — for alias case that's aliased_name,
               so match-by-resolved-identity works naturally without a separate branch)
                                                    │
                                                    ▼
              For each design-tier match onto a source-tier survivor:
                accumulator[survivor_idx].ranges.push(design.range)
                accumulator[survivor_idx].manifests.push(design.source_manifest)
                accumulator[survivor_idx].declared_as.extend(design.declared_as)
                                                    │
                                                    ▼
              Post-transfer:
                Sort accumulator[i].manifests lex; reorder ranges 1:1
                Sort + dedup accumulator[i].declared_as lex
                Stamp survivor's extra_annotations:
                  mikebom:requirement-ranges (array)
                  mikebom:source-manifests (array)
                  mikebom:declared-as (array, only if non-empty)
```

## Cross-Cutting: FR-008 byte-identity implications

**Empirical finding** (research §R4): Zero existing goldens in the corpus reference `"mikebom:requirement-range"` or `"mikebom:source-manifest"` — the m191 reconciler survivor code path was never exercised by any pre-m199 fixture. Consequence: no existing golden requires regen. Only net-new goldens for the m199 US1/US2 fixtures land in the PR.
