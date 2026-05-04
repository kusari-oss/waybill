# Data Model: maven source-tree main-module component

## Entities

### MavenMainModuleEntry (no new Rust type — constrained `PackageDbEntry`)

| Field | Value | Source | FR |
|-------|-------|--------|-----|
| `purl` | `pkg:maven/<groupId>/<artifactId>@<version>` | resolved GAV via `build_maven_purl` | FR-001 |
| `name` | `<artifactId>` | POM `<artifactId>` | FR-001 |
| `version` | resolved version (literal or property-substituted or `<parent>`-inherited) | POM `<version>` or inheritance | FR-001 |
| `source` | `Some("path+file://<absolute-pom-dir>")` | filesystem walker | (existing convention) |
| `lifecycle_scope` | `None` | n/a | (out of scope) |
| `sbom_tier` | `Some("source")` | constant | FR-006 |
| `extra_annotations` | BTreeMap with `mikebom:component-role: "main-module"` | constant | FR-004 |
| `parent_purl` | `None` (NOT to be confused with POM's `<parent>` block — that's GAV inheritance, not SBOM hierarchy) | constant | FR-001a |
| `depends` | `Vec<String>` from POM `<dependencies>` block (resolved-version GAVs as strings) | POM | FR-007 |
| `licenses` | `vec![]` | constant | FR-005 |
| `hashes` | `vec![]` | constant (synthetic) | (n/a) |

### PomXmlDocument extension (existing struct, new field)

```rust
pub(crate) struct PomXmlDocument {
    pub self_coord: Option<(String, String, String)>,
    pub parent_coord: Option<(String, String, String)>,
    pub properties: HashMap<String, String>,
    pub dependencies: Vec<PomDependency>,
    pub dependency_management: Vec<PomDependency>,
    pub self_artifact_id: Option<String>,
    pub modules: Vec<String>,                  // ⬅️ NEW for milestone 070
}
```

`modules` is populated by the event-driven `parse_pom_xml` walker when it sees `<modules>/<module>` elements. ~10 LOC parser extension. Each `<module>` value is a relative directory path (e.g., `module-a`, `submodules/foo`) that contains a child `pom.xml`.

### MavenDroppedDuplicate (private helper struct)

Same shape as cargo (064) / npm (066) / pip (068) / gem (069). Returned from `dedup_maven_main_modules_by_purl`.

### Property substitution helper

```rust
fn resolve_pom_property_value(
    raw_value: &str,
    self_doc: &PomXmlDocument,
    parent_doc: Option<&PomXmlDocument>,
) -> ResolvedValue {
    // ...
}

enum ResolvedValue {
    Literal(String),     // value didn't contain ${...}
    Resolved(String),    // ${...} successfully resolved
    Unresolved(String),  // ${...} couldn't be resolved → emit verbatim + warn
}
```

Resolves `${project.groupId}`, `${project.artifactId}`, `${project.version}`, `${parent.groupId}`, `${parent.version}`, `${revision}`, and any custom keys from `self_doc.properties` (preferred) or `parent_doc.properties` (fallback per Maven inheritance). Per FR-012, NO Maven runtime, NO settings.xml, NO active-profile resolution.

### POM inheritance context

```rust
struct MavenInheritanceContext {
    /// Map from (groupId, artifactId, version) coordinate → parsed
    /// `PomXmlDocument` for every POM discovered in the scan tree.
    /// Built upfront so child POMs can look up their parent without
    /// re-parsing.
    by_coord: HashMap<(String, String, String), PomXmlDocument>,
}
```

When resolving a child POM's GAV:
1. If `<groupId>` and `<version>` are both present in the child → no inheritance needed.
2. If either is missing AND `<parent>` block declares one → use the parent's coordinate from the `<parent>` block's literal text (ALWAYS available since `<parent>` requires complete GAV).
3. If `<parent>` is absent but `<groupId>` or `<version>` is missing → invalid POM; skip emission.

## Relationships

### Direct-dep edges

```text
Relationship {
    from: <maven-main-module-purl>,
    to: <dep-target-purl>,
    relationship_type: DependsOn,
    provenance: {
        source: "<absolute-pom.xml-path>",
        data_type: "maven-pom-direct-dep",
    },
}
```

Existing maven dep-emission machinery emits these via the existing edge-emission loop in `scan_fs/mod.rs`.

### DESCRIBES relationship

Inherits multi-DESCRIBES wiring from milestone 064 + #127. Multi-module reactor → length-N `documentDescribes`.

### Multi-module reactor traversal

Parent's `<modules>` lists subdirectories. Each subdirectory's `pom.xml` is read separately and produces its own main-module via FR-002. The parent's main-module emits independently if its own GAV is complete (Edge Cases: bare aggregator parent without own GAV → skip parent, still emit per-submodule).

## State transitions

None.

## Validation rules

| Rule | Source | Failure mode |
|------|--------|--------------|
| `pom.xml` MUST be parseable XML | FR-001 | Skip with warn |
| `<artifactId>` MUST be present in POM body (not from `<parent>`) | FR-001 step 5 | Skip silently |
| Missing `<groupId>` or `<version>` resolved from `<parent>` block | FR-001 step 2 | Skip if `<parent>` absent too |
| `${...}` properties resolved per FR-012 set | FR-012 | Verbatim + warn if unresolvable |
| `target/`, `.m2/`, `node_modules/` excluded | FR-003 | Walker skips |
| Same-PURL collisions dedup | FR-011 | First-discovered wins; warn |
| Bare aggregator parent (only `<modules>`, no own GAV) skipped | Edge Cases | Submodules still emit |

## Reuses from milestones 053+064+066+068+069+#127 + existing maven.rs

- `parse_pom_xml` (existing `maven.rs:570`) — XML parser via `quick-xml`
- `parse_pom_properties` (existing `maven.rs:1062`) — POM properties parser
- `build_maven_purl` (existing `maven.rs:1793`) — PURL builder with `/` separators
- C40-tag-driven CDX `metadata.component` selector (milestone 064)
- C40-tag-driven SPDX `primaryPackagePurpose` predicate (milestone 053+064)
- Multi-root `documentDescribes` + per-root DESCRIBES (#127)
- Multi-root SPDX 3 `rootElement` + per-root describes (#127)
- Cargo's `dedup_main_modules_by_purl` pattern (milestone 064 T010)

## Does NOT introduce

- No new public Rust type
- No new crate dependency (`quick-xml` already in tree)
- No new CLI flag
- No new SBOM annotation key
- No subprocess calls (no Maven runtime)
- No settings.xml / `~/.m2/` reading for main-module resolution (unlike the dep-emission path which reads `~/.m2/repository/` for installed-artifact identification)
- No `<profiles>` activation
