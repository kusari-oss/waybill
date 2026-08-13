# Contract: `cache_reader.rs` (US2 cache reader)

**File**: `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs`
**Consumers**: `ladder.rs`

---

## Entry point

```rust
pub fn resolve_via_cache(
    project_dir: &Path,
    declared_coords: &[MavenCoord],
) -> Result<GradleResolvedGraph, GradleCacheError>;
```

`declared_coords` comes from a shallow static parse (a lightweight
predecessor to US3) that extracts the direct-dep coordinates from
`build.gradle(.kts)` so the cache reader has a starting set to
transitively expand from.

## Contract

1. **Discover cache root**: Prefer `$GRADLE_USER_HOME/caches/modules-2/`;
   fall back to `${HOME}/.gradle/caches/modules-2/`. If neither
   exists, return `GradleCacheError::CacheAbsent`.

2. **Pick metadata-2 directory**: Enumerate subdirs matching
   `metadata-2.*`; prefer the highest-suffix (`metadata-2.107` >
   `metadata-2.106`). If none match, return `CacheAbsent`.

3. **Per declared coord**:
   - Path: `<metadata_dir>/descriptors/<group>/<artifact>/<version>/`.
   - If the directory doesn't exist, record `CacheMiss` for this
     coord and continue (partial coverage; final decision to
     degrade is up to the ladder based on threshold).
   - Prefer reading `<coord>.module` (Gradle Module Metadata JSON)
     when present; fall back to `<coord>.pom` (XML) otherwise.

4. **Parse POM** (quick-xml, matches `maven.rs` pattern):
   - Extract `<dependencies>/<dependency>` entries.
   - For each, resolve `<groupId>:<artifactId>:<version>` to a
     `MavenCoord` and recurse.
   - Cycle detection: maintain a `HashSet<MavenCoord>` of
     already-processed coords; skip re-descent on cycle.

5. **Parse `.module` JSON** (serde_json):
   - Read `variants[]` array; pick the `runtime` variant (or
     `apiElements`+`runtimeElements` merged).
   - Extract `dependencies[]` from the chosen variant.
   - Recurse per-dep as with POM.

6. **Cache freshness annotation**:
   - Compare the newest mtime among the read cache entries against
     the `build.gradle(.kts)` mtime.
   - If cache is newer → emit `waybill:cache-freshness = "fresh"`
     on the emitted `GradleResolvedGraph`.
   - Else → `waybill:cache-freshness = "stale"` (attach to the
     document-scope annotations later at emission time).

7. **Success**: return `GradleResolvedGraph { tier: Cache, ... }`.

8. **Coverage threshold**: If more than 30% of declared coords hit
   `CacheMiss`, return `GradleCacheError::InsufficientCoverage {
   miss_count, total_count }` — the ladder decides to degrade
   based on this signal.

## Failure modes

| Condition | Outcome |
|---|---|
| No cache directory found | `CacheAbsent` |
| Cache exists but declared coords missing (>30%) | `InsufficientCoverage` |
| POM XML malformed | `PomParseError` (skip that coord, continue) |
| `.module` JSON malformed | `ModuleParseError` (skip that coord, continue) |
| Success | `Ok(graph)` |

## Test hooks

- Cache-root override via `WAYBILL_TEST_GRADLE_CACHE=<path>` env
  var (only respected in `#[cfg(test)]` builds).
- Golden POM + `.module` fixtures at
  `waybill-cli/tests/fixtures/golden_inputs/gradle/no_wrapper_warm_cache/gradle-caches-fixture/`
  mirror the real Gradle cache layout.
