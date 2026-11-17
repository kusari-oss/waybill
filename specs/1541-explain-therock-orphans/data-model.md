# Data model: every unreachable component on TheRock explains itself

Everything is in process for one scan.

## `graph_completeness::for_document` (new)

```rust
pub struct DocumentSubject<'a> {
    pub root_override: &'a RootComponentOverride,
    pub scan_target_coord: Option<&'a ScanTargetCoord>,
    pub target_name: &'a str,
    pub target_version: &'a str,
    pub target_kind: ScanTargetKind,
}
pub fn for_document(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    subject: &DocumentSubject<'_>,
    unread_manifests: UnreadManifests<'_>,
) -> GraphCompletenessResult
```

This is the emitters' preparation, extracted once:
- root selection;
- `effective_components`;
- the anchoring rewrites;
- workspace-peer edges;
- **containment edges, `parent_purl` → child (R2).**

The three emitters and `classify_orphans_pre_emit` all call it. The `reachable_set` it returns is the one set the classifier, the document and I7 use.

## C45 additions (R3)

| code | emitted when | priority |
|---|---|---|
| `test-data-binary` | unreachable; derived from binaries; every source file under a test-data directory (closed list, R3) | below every existing code |
| `unowned-binary` | unreachable; derived from binaries; not every source file under a test-data directory | below `test-data-binary` |

"Derived from binaries" means `waybill:sbom-tier` = `analyzed`, or every source file is one the binary scanner read. The exact predicate is fixed at T-impl from the TheRock population (`source-trace.md`), and pinned by a contract case per branch.

## uv reader (R4)

- **An editable root with no `version`** is emitted versionless, with C151 `waybill:unresolved-reason` = `dynamic version in pyproject.toml; not resolved from the lock`.
- **The root's `dependencies`** are runtime edges.
- **`optional-dependencies`** are optional-scope edges.
- **`dev-dependencies`** are development-scope edges.
- **Every listed version of a forked dependency** gets an edge.

## I7 check (test harness)

`orphan_explained::check` walks nested components, and treats a component as reachable when its container is.
