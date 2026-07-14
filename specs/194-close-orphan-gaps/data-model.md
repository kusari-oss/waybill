# Data Model: m194 Close Remaining Orphan Gaps

**Date**: 2026-07-14
**Scope**: In-process types + edit shapes touched by both user stories. Zero cross-crate type changes.

## Entity: `PackageDbEntry` (existing, EXTENDED per US2)

Located in `mikebom-cli/src/scan_fs/package_db/mod.rs`. No struct-level change. The US2 fix appends new entries with:

- `purl`: `pkg:npm/<sanitized-dir-basename>` (versionless PURL per m191 convention)
- `name`: `<sanitized-dir-basename>` (per-purl-spec charset)
- `version`: `""` (empty, matching m191's versionless-PURL emitter behavior)
- `sbom_tier`: `Some("source")` (matches existing top-level npm mainmod)
- `extra_annotations`: `{"mikebom:component-role": "main-module"}` (drives root selection)
- `depends`: derived from the nameless manifest's `dependencies` + `optionalDependencies` + optionally `devDependencies` (via `include_dev`)
- `source_path`: `format!("path+file://{}", nested_project_root.display())` — matches m066 convention

## Function: `build_stdlib_entry` caller (existing, EXTENDED per US1)

**Location**: `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:2256`.

**Existing behavior**: iterates parsed_roots + emits one `PackageDbEntry` per unique Go version via `build_stdlib_entry`.

**Extended behavior**: after each stdlib entry is pushed, find the corresponding Go mainmod component (the one whose `mikebom:component-role: main-module` annotation is set AND whose `.evidence.source_file_paths` includes the go.mod file of the same project root), and append `"stdlib"` to its `.depends` list.

Pseudocode:

```rust
if let Some(entry) = build_stdlib_entry(bare, &source_path_for_evidence) {
    if emitted_versions.insert(bare.to_string()) {
        entries.push(entry);
    }
    // Milestone 194 (US1 / FR-001): link the primary Go mainmod for
    // this project_root to the stdlib entry, so BFS reachability
    // includes stdlib. Reuses the existing name → PURL Relationship
    // emission at `scan_fs/mod.rs:756-772`.
    let mainmod_source_path = format!("path+file://{}", project_root.display());
    for e in entries.iter_mut() {
        let is_go_mainmod = e.purl.as_str().starts_with("pkg:golang/")
            && e.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
            && e.source_path == mainmod_source_path;
        if is_go_mainmod && !e.depends.iter().any(|d| d == "stdlib") {
            e.depends.push("stdlib".to_string());
        }
    }
}
```

**Fallback for name-ambiguity (per research R4)**: if `name_to_purl` at `scan_fs/mod.rs:756` produces ambiguous mapping when multiple `pkg:golang/stdlib@vX` and `pkg:golang/stdlib@vY` exist under the same name `"stdlib"`, extend `PackageDbEntry.depends` to accept a PURL-string entry (backwards-compat: names still work). Investigate at implementation time; if ambiguous, use direct-Relationship-emit as a fallback.

## Function: `apply_nameless_secondary_umbrella` companion (NEW per US2)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/mod.rs`, adjacent to existing `apply_nameless_secondary_umbrella` at line 361.

**Purpose**: Synthesize a mainmod component for each nameless nested `package.json` that was NOT already handled by the m066 mainmod-emission loop OR the m256 umbrella pass.

**Behavior**:

```rust
fn synthesize_nameless_nested_mainmods(
    rootfs: &Path,
    include_dev: bool,
    entries: &mut Vec<PackageDbEntry>,
    exclude_set: &super::exclude_path::ExclusionSet,
) {
    let project_roots = candidate_project_roots(rootfs, exclude_set);
    let existing_mainmod_dirs: HashSet<PathBuf> = entries
        .iter()
        .filter(|e| /* is npm main-module */)
        .filter_map(|e| e.source_path.strip_prefix("path+file://").map(PathBuf::from))
        .collect();
    for project_root in &project_roots {
        // Skip if a mainmod already exists for this dir.
        if existing_mainmod_dirs.contains(project_root) { continue; }
        // Skip if package.json has a `name` field (handled by m066).
        let manifest_path = project_root.join("package.json");
        let parsed = /* read + parse */;
        if parsed.get("name").is_some() { continue; }
        // Collect declared dep names.
        let dep_names = collect_declared_dep_names(&parsed, include_dev);
        if dep_names.is_empty() { continue; }
        // Synthesize mainmod PURL from dir basename.
        let basename = project_root.file_name().and_then(|s| s.to_str()).unwrap_or("nameless-workspace");
        let purl_str = format!("pkg:npm/{}", encode_purl_segment(basename));
        let Ok(purl) = Purl::new(&purl_str) else { continue; };
        // Build the mainmod entry.
        let mut entry = PackageDbEntry { /* ... */ };
        entry.extra_annotations.insert(
            "mikebom:component-role".to_string(),
            json!("main-module"),
        );
        entry.depends = dep_names;
        entries.push(entry);
        synthesized_count += 1;
    }
    if synthesized_count > 0 {
        tracing::info!(
            synthesized_count,
            "npm: synthesized nameless-nested-workspace main-modules"
        );
    }
}
```

Called from `npm/mod.rs::read` after `apply_nameless_secondary_umbrella` (line 302), before dedup.

## Entity: Synthetic stdlib DependsOn edge (NEW, ephemeral)

Produced by `scan_fs/mod.rs:756-772` when it walks the Go mainmod's `.depends` list (which now includes `"stdlib"` per US1). Shape identical to any other DependsOn edge from that emission path:

```rust
Relationship {
    from: <Go mainmod PURL>,
    to: <pkg:golang/stdlib@v<version>>,
    relationship_type: RelationshipType::DependsOn,
    provenance: EnrichmentProvenance {
        source: <Go mainmod source_path>,
        data_type: "package-database-depends",
    },
}
```

No new provenance shape — reuses the existing "package-database-depends" provenance the outer emit loop assigns to all Go DependsOn edges. Traceable back to the mainmod source_path.

## Entity: Synthetic nested-workspace mainmod (NEW, persistent in components)

The `PackageDbEntry` synthesized by `synthesize_nameless_nested_mainmods`. Shape covered above under "PackageDbEntry EXTENDED per US2". Persists in the emitted SBOM as a normal `pkg:npm/<basename>` component with `mikebom:component-role: main-module` — indistinguishable from top-level npm mainmods except for the versionless PURL shape.

## Cross-format contract

Both US1 and US2 changes flow through the same emit-time pipeline. The synthetic stdlib edge appears in CDX `dependencies[]`, SPDX 2.3 `relationships[]` DEPENDS_ON, and SPDX 3 `Relationship` graph elements identically per the existing `Relationship` plumbing. The synthetic nested mainmod appears in CDX `components[]`, SPDX 2.3 `packages[]`, and SPDX 3 `software_Package` graph elements per the existing component emission — with the versionless PURL shape per m191.

## Downstream classifier consequence

- Before m194: Go source scans reported 1 orphan (stdlib); nested-nameless-npm scans reported N orphans (transitive dep count).
- After m194: both classes reachable via BFS from primary root (or from operator's `target_ref` under `--root-name` per m192/m193 pre-rewrite). If NO other orphans exist, `mikebom:graph-completeness == "complete"`. Real orphans continue to fire `OrphanedComponentsDetected` per FR-016.
