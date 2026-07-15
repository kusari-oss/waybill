# Data Model: m190 + m191 Follow-Up Bundle

**Date**: 2026-07-15
**Purpose**: Enumerate the concrete data-shape changes m197 applies. Reuses existing types wherever possible; introduces one new annotation (`mikebom:declared-as`) and rotates two existing annotations from scalars to arrays.

## Reused Entities

- **`mikebom_common::types::purl::Purl`** — the newtype used by all 11 emitters. No shape changes; m197 exercises the versionless code path more thoroughly (US3 + US4 fuzz test).
- **`PackageDbEntry.extra_annotations: BTreeMap<String, serde_json::Value>`** — the per-component annotation carrier all readers write to. m197 rewrites two entries (`requirement-range` / `source-manifest`) and adds one (`declared-as`).
- **`Reconciler::reconcile()`** at `mikebom-cli/src/resolve/reconciler.rs` — the m191 reconciler entry point. Extended, not replaced.
- **`AliasResolution`** at `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` — the m159 alias record. Extended with a `declared_as: String` field (or renamed if a natural field already covers this).

## New / Rotated Entities

### E1. `mikebom:declared-as` annotation (US5)

**Location**: On any reconciler-survivor component that consumed one or more design-tier hits declared via an npm alias.

**Shape**: JSON array of strings — the ORIGINAL alias name(s) as written in the source manifest. Multiple entries when a monorepo has the same resolved dep declared via different aliases in sibling manifests (edge case in spec).

**Example**:
```json
{
  "extra_annotations": {
    "mikebom:declared-as": ["my-preferred-name"]
  }
}
```

Or with multi-manifest aliases:
```json
{
  "extra_annotations": {
    "mikebom:declared-as": ["my-preferred-name", "legacy-alias"]
  }
}
```

**Validation rules**:
- Array MUST be non-empty when the annotation is present (empty array is an emission bug).
- Values are sorted lexicographically for deterministic goldens.
- **Duplicate alias values ARE deduped** — unlike E2/E3 which preserve duplicates for count-provenance signal (`3 manifests declared this range`), an alias is a name-mapping and the same alias declared twice conveys no additional information. So `["my-alias", "my-alias"]` sourced from two manifests collapses to `["my-alias"]`.
- Values are the raw alias strings from the manifest — NO npm-namespace prefixing, NO version suffix (alias-vs-resolved is a NAME mapping, not a version mapping).
- Annotation is emitted ONLY when at least one alias was involved; components with no alias declarations have no `mikebom:declared-as` key.

### E2. `mikebom:requirement-ranges` annotation (US6 — rotation from m191 singular)

**Location**: On any reconciler-survivor with at least one design-tier match. Present on every survivor (both single-manifest and multi-manifest cases per Q1 always-array decision).

**Shape**: JSON array of strings — the declared requirement ranges from EACH design-tier hit reconciled onto the survivor.

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
- Array MUST be non-empty when the annotation is present.
- Values are the raw range strings from the manifest — no normalization / no dedup even if two manifests declare the same range (preserving provenance per US6).
- Order MUST correspond 1:1 with `mikebom:source-manifests` — the Nth range came from the Nth manifest. Ordering rule: lexicographic by manifest path (deterministic goldens).

### E3. `mikebom:source-manifests` annotation (US6 — rotation from m191 singular)

**Location**: Same as E2 — on every reconciler-survivor with at least one design-tier match.

**Shape**: JSON array of strings — the source-manifest paths (or manifest URIs) each contributing declaration came from.

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
- Array MUST be non-empty when the annotation is present.
- Values are workspace-relative paths (matching the m191 singular format).
- Sorted lexicographically. Ordering 1:1 with `mikebom:requirement-ranges` (E2).

### E4. Removed: `mikebom:requirement-range` + `mikebom:source-manifest` (singular scalars)

**Post-m197**: These field names NO LONGER APPEAR on reconciler-survivor components. Consumers keying on the singular names will need to migrate to the plural array shapes (E2/E3). This is the ONLY breaking-change class in m197 per FR-007's Q1 exception.

**Migration guidance** (for external consumers): both singular names had exactly one value; the array replacements are 1-element in the same case. Migration is `annotation.value` → `annotation.value[0]`.

## Rotated Entity: PURL emission for the 6 US3 ecosystems

**Pre-m197 shape** (broken):
- `pkg:composer/vendor/pkg@` (trailing `@`, invalid per purl-spec)
- `pkg:pub/dartpkg@` (same)
- Analogous for cocoapods / scala / haskell / erlang

**Post-m197 shape** (per US3 + purl-spec canonical):
- `pkg:composer/vendor/pkg` (no `@` when version absent)
- `pkg:pub/dartpkg` (same)
- Analogous for cocoapods / scala / haskell / erlang

Emission of the versioned form is unchanged (`pkg:composer/vendor/pkg@1.0.0` etc.) per FR-007.

## Rotated Entity: PURL emission for epoch-versioned dpkg + apk (US1 + US2)

**Pre-m197 shape** (US1/US2 broken):
- `pkg:deb/debian/foo@1:2.0-r0` (epoch inline in version segment, invalid per purl-spec)
- `pkg:apk/alpine/bar@1:2.0-r0` (same)

**Post-m197 shape**:
- `pkg:deb/debian/foo@2.0-r0?epoch=1`
- `pkg:apk/alpine/bar@2.0-r0?epoch=1`

Emission for non-epoch packages is byte-identical to pre-m197 output.

For rpm (US2b), the m003/m004/m144 code path already produces the correct `?epoch=<N>` qualifier form; audit result recorded in `scratch/rpm-audit-findings.txt`, no code change unless the audit surprises.

## State Transitions

The reconciler flow (updated for US5 + US6):

```text
[Design-tier component]
  ├── ecosystem = npm ─── alias declaration? ─── Yes ── stamp mikebom:declared-as = <alias name>
  │                                                              │
  │                                                              ▼
  └────────────────────────────────────────────────  Reconciler.reconcile()
                                                                 │
                                                                 ▼
                          match-key: (ecosystem, name_or_resolved_identity, source-manifest-dir)
                          alias-aware: if declared-as present, match by resolved identity
                                                                 │
                                                                 ▼
                          Survivor accumulates:
                          - mikebom:requirement-ranges += [design.range]         (E2 — always array)
                          - mikebom:source-manifests += [design.source-manifest] (E3 — always array)
                          - mikebom:declared-as += [design.declared-as]          (E1 — only if present)
                                                                 │
                                                                 ▼
                          Final sort:
                          - source-manifests sorted lex; ranges reordered 1:1 to match
                          - declared-as sorted lex
```

## Cross-Cutting: Byte-identity implications

Every existing golden that grepped positive for `"mikebom:requirement-range"` or `"mikebom:source-manifest"` (singular) requires regeneration to convert to the array form. Every OTHER golden holds byte-identically per FR-007.
