# Contract — Pants coursier-JVM reader behavior post-fix

**Module**: `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`

## Invariants the fix must uphold

### Input tolerance (`directDependencies` field — unmodeled)

The reader MUST parse every lockfile containing one of these `directDependencies` shapes (or any mix within the same lockfile):

| Shape | TOML rendering | Post-fix behavior |
|---|---|---|
| Absent | field not present in `[[entries]]` block | parse OK — field silently ignored |
| Empty array | `directDependencies = []` | parse OK — field silently ignored |
| String array (legacy) | `directDependencies = ["group:artifact:version", ...]` | parse OK — field silently ignored |
| Coord-table array | `[[entries.directDependencies]] { group, artifact, version }` | parse OK — field silently ignored (WAS: reject whole lockfile) |
| Any future shape | any valid TOML value at this key | parse OK — field silently ignored |

### Input tolerance (`dependencies` field — `Vec<DependencyRef>` untagged enum)

The reader MUST parse every lockfile containing one of these `dependencies` shapes (or any mix within the same lockfile). Unlike `directDependencies`, the parsed value IS consumed downstream (feeds dep-graph edges), so the field is modeled via an untagged enum.

| Shape | TOML rendering | Post-fix behavior | Dep-edge extraction |
|---|---|---|---|
| Absent | field not present | parse OK — field defaults to empty Vec | zero edges |
| Empty array | `dependencies = []` | parse OK — empty Vec | zero edges |
| String array (legacy) | `dependencies = ["group:artifact:version", ...]` | parse OK — `DependencyRef::String` variants | edges via `parse_coord_string` (existing path) |
| Coord-table array | `[[entries.dependencies]] { group, artifact, version }` | parse OK — `DependencyRef::CoordTable` variants (WAS: reject whole lockfile) | edges via direct field extraction |
| Malformed coord-table entry | e.g., missing required `group` field | serde parse fails for that variant; edge dropped with WARN | one edge lost, others continue |

### Output shape

The reader MUST emit output that is **byte-identical** to pre-fix output for any lockfile that parsed successfully pre-fix. Concretely:

- PURL construction is unchanged.
- `depends_on` edges derived from `dependencies[]` (the sibling field carrying the transitive graph) are unchanged.
- Per-entry annotations (`waybill:pants-resolve`, `waybill:source-url`) are unchanged.
- Structured log fields (`lockfiles_discovered`, `lockfiles_parsed_ok`, `lockfiles_skipped_corrupt`, `components_emitted`) report the same values for pre-fix passing fixtures.

### Failure mode changes

| Pre-fix behavior | Post-fix behavior | Reason |
|---|---|---|
| Whole-lockfile skip on coord-table `directDependencies` | Parses successfully | The field is now invisible to the struct |
| Individual entry with `coord.group == ""` → WARN + skip entry (existing) | Same behavior | No change (validated by new test T-E) |
| Individual entry with PURL construction failure → WARN + skip entry (existing) | Same behavior | No change |
| TOML syntax error at any point in the file → whole-lockfile skip (existing) | Same behavior | No change |

## What the fix MUST NOT do

- MUST NOT introduce any new `waybill:*` annotation, property, or relationship type. This is a bug fix; no new emission surface is in scope.
- MUST NOT alter the PURL shape for any Maven component.
- MUST NOT alter dependency edge shape (source, target, edge type).
- MUST NOT change lifecycle-scope classification for any component.
- MUST NOT change the WARN log format for entry-level failures.
- MUST NOT introduce any new Cargo dependency.
- MUST NOT touch `waybill-common/` or `waybill-ebpf/`.
- MUST NOT touch any reader other than `pants_jvm/lockfile.rs`.

## Behavioral contract for the corpus target

The re-enabled `pants-example-jvm` corpus target commits to:

- **Layer 1**: assertion function `pants_example_jvm_layer1` runs 4 invariants (data-model §Entity 6). Failure produces a diagnostic naming the coursier-JVM reader (`m224 / issue #676`) as the suspected regression site.
- **Layer 2**: full-SBOM byte-identity goldens (no per-target filter needed — the fixture is single-ecosystem JVM). Regenerating goldens requires `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`.
- **Manifest audits**: `public_only_audit`, `public_hostname_allowlist`, `no_credentials_required`, `cross_ecosystem_coverage_check` — all pass (the existing `Ecosystem::JavaMaven` is already used by `maven-guice`).

## Regression signal the fix restores

Before this fix: scanning any Pants JVM monorepo produced **zero** `pkg:maven/*` components.

After this fix: scanning `pantsbuild/example-jvm` at the pinned SHA produces ≥ 20 `pkg:maven/*` components including the four top-level coords declared in the fixture's PANTS LOCKFILE METADATA header.

## Non-goals

- **Not fixing**: any other coursier lockfile parse quirk that isn't the `directDependencies` shape mismatch. If such a quirk surfaces, file separately.
- **Not fixing**: any downstream classification of "direct vs transitive" for JVM components. The `direct_dependencies` field was declared for future use; that future use (if it ever materializes) is out of scope.
- **Not adding**: pnpm/yarn/nuget/etc coverage improvements. This is JVM-only.
