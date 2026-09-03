# Research — Fix m224 coord-table `directDependencies` parse (issue #756)

## R1 — Option A vs Option B: is `direct_dependencies` used downstream?

**Question**: Issue #756 suggests two fix paths — (A) `serde` untagged enum accepting both shapes; (B) drop `directDependencies` parsing entirely. Which is correct depends on whether the parsed value is consumed by any downstream code in the reader.

**Investigation**: `grep -rn "direct_dependencies\|directDependencies" waybill-cli/src/scan_fs/package_db/pants_jvm/` returns 4 non-test hits in production code:

| Site | Purpose |
|---|---|
| `lockfile.rs:59-60` | Struct field declaration: `#[serde(default, rename = "directDependencies")] pub(crate) direct_dependencies: Vec<String>,` |
| `lockfile.rs:355-358` | Explicit dead-code sink with comment: `// Silence dead-code warnings for direct_dependencies + file_name + serialized_bytes_length; the fields are declared for schema documentation and future use per data-model.md.\nlet _ = &entry.direct_dependencies;` |
| `lockfile.rs:497` | Test fixture initialization: `direct_dependencies: Vec::new(),` (unit test constructor) |
| `coordinate.rs:3` | Documentation-only comment: `//! Coursier's dependencies[] / directDependencies[] fields hold ...` |

**Cross-referenced**: `dependencies` (the sibling field carrying the transitive dep graph) is actively used at `lockfile.rs:318` where its values feed the `depends` Vec that becomes the SBOM's `depends_on` edges.

**Findings**:

- `direct_dependencies` is **explicitly dead code** at emission time. The comment at line 355 declares the field is "declared for schema documentation and future use" — parsed but ignored.
- `dependencies` is the load-bearing field for the dep graph. It is unchanged by this fix.
- No downstream code (annotations, edges, classifications, tests) reads `direct_dependencies`.
- The "direct vs transitive" classification the spec speculated about does NOT exist in the reader today. The concept lives only in the doc-comment at `coordinate.rs:3`.

**Decision**: Option B for `direct_dependencies` (delete field). During implementation, discovered that the sibling `dependencies` field has the same bug shape but IS load-bearing, so it received a different fix: an untagged enum `DependencyRef` accepting both string and coord-table forms. See **R1 amendment** below.

## R1 amendment (2026-09-03 implement-time discovery)

**Discovery**: T008 (US1 empirical smoke-scan) rejected on line 83 of the fixture with the same "coursier TOML body parse error" WARN after only the `directDependencies` fix landed. The sibling `dependencies` field ALSO uses the coord-table shape in real-world Pants coursier lockfiles (e.g., `[[entries.dependencies]] { group, artifact, version }` at line 94 of `pantsbuild/example-jvm`'s lockfile).

**Amended decision**:

- `direct_dependencies`: DELETE the field (unchanged — the parsed value was never consumed downstream).
- `dependencies`: KEEP the field but change its element type from `Vec<String>` to `Vec<DependencyRef>` where:

  ```rust
  #[derive(Debug, Deserialize)]
  #[serde(untagged)]
  pub(crate) enum DependencyRef {
      String(String),
      CoordTable { group: String, artifact: String, ... },
  }
  ```

- The mapper at `entry_to_package_db_entry` iterates `dependencies` and produces the `depends: Vec<String>` for dep-graph edges. It now handles both variants: `String` case parses via existing `parse_coord_string`; `CoordTable` case extracts `group` + `artifact` directly (no intermediate parse).

**Why the split-fix**: `direct_dependencies` is dead code — deleting it is pure simplification. `dependencies` carries the dep graph — deleting it would silently regress edge emission. The load-bearing field justifies the modest complexity of an untagged enum.

**Production diff post-amendment**: ~40 lines (up from research's ~5-10 estimate). Still comfortably under SC-007's 100-line ceiling.

## R1 (original, retained for context)

**Rationale**:

1. **Zero behavior change** — the field is unused; deleting it removes no observable emission.
2. **Simpler fix** — one field deletion + one dead-code sink line deletion, vs an untagged enum + coord-table struct + serde variant handling.
3. **Bigger robustness win** — with the field gone, the reader ignores WHATEVER shape upstream Pants versions choose for `directDependencies`. An untagged enum only handles the specific shapes we predict; a deletion handles every shape past, present, and future.
4. **Follows Constitution Principle IV (Type-Driven Correctness)** — removing an unused type simplifies the type; adding a variant that's discarded adds a type without semantic meaning.
5. **Fits SC-007 (≤ 100 line diff)** — this approach lands in ~5-10 lines of production code.

**Alternatives considered**:

- **Option A (untagged enum)**: rejected. Adds a `DepRef` enum + a `CoordTableEntry` struct + serde deserialize wiring — all to store a value nothing consumes. Preserves an artifact of the reader's original design that was never wired to any emission surface.
- **Option A-mini (change field type to `serde_json::Value`)**: also rejected but less-bad than A. Would accept any shape without the enum complexity. Still stores a value nothing reads. Rejected in favor of straight deletion.
- **Option C (preserve field but wire it to emission)**: out of scope. Emitting a `waybill:direct-dependency` annotation or similar would be a new feature, not a bug fix. If direct-vs-transitive classification for JVM becomes a requirement, that's a separate follow-up milestone (parallel to m180 for npm).

**Reference note**: The spec's Assumptions section explicitly parked this decision at planning-time with the condition "if fixture-in-code inspection reveals the field is unused downstream, planning may adopt option B instead." The condition is met.

## R2 — Existing tests that will need updates

**Question**: What existing test-fixture initializations construct `Entry` instances with an explicit `direct_dependencies` field, and will those break after the field is removed?

**Investigation**: `grep -n "direct_dependencies" waybill-cli/` and `grep -n "directDependencies" waybill-cli/tests/`.

**Findings**:

| Site | Update needed |
|---|---|
| `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs:497` (in-file `parse_valid_pants_coursier_lockfile` test) | Remove `direct_dependencies: Vec::new(),` line. |
| `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs:411, 441` (TOML fixture strings inside tests) | Existing test lockfile strings contain `directDependencies = []`. Since serde `#[serde(deny_unknown_fields)]` is NOT set on `Entry` (verified from struct declaration), these TOML strings will parse fine after the field is removed — TOML fields not matching struct fields are silently ignored by default. No update needed. |
| `waybill-cli/tests/pants_coursier_jvm_reader.rs` | Grep returned zero hits for direct_dependencies in integration tests. No update needed. |

**Decision**: Update `lockfile.rs:497` only. All other test artifacts are compatible with the field-deletion path.

## R3 — Entry-level fail-open per FR-004

**Question**: FR-004 requires per-entry warn-and-skip on malformed coord data. How is this implemented in the existing code?

**Investigation**: Read `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` `entry_to_package_db_entry` function (lines 232-403) — the per-entry conversion path.

**Findings**:

- Existing code already has per-entry validation at lines 237-247: `group`/`artifact`/`version` trimmed; if any is empty, log WARN + return `None` (skips this entry, continues with others).
- Existing code already has per-entry PURL construction failure handling (lines 251-260): PURL::new() failures are logged + skipped.
- **FR-004's requirement is already satisfied by the existing code.** No entry-level fail-open changes needed by this fix.

**Decision**: No production code change for FR-004. Add a unit test (per FR-007 clause d) verifying the existing behavior on a malformed coord to lock in the current fail-open contract.

## R4 — Corpus target re-enablement

**Question**: What's the shape of the corpus target for `pants-example-jvm`?

**Investigation**: Read `waybill-cli/tests/corpus_harness_195/manifest.rs` — the current comment block that documents the deferred JVM target from PR #757.

**Findings**:

- PR #757 landed a comment reserving the slot: `NOTE: pants-example-jvm intentionally omitted for now — the m224 reader rejects the coord-table form of directDependencies used by real Pants coursier lockfiles (#756). Fork is ready at kusari-sandbox/example-jvm at SHA 675ee75d36f2c1b096b0def51efcfffd02bd1251; add the entry back once #756 is resolved.`
- Fork already exists (verified during PR #757 T004).
- Ecosystem tag: `Ecosystem::JavaMaven` per the m195 manifest enum (matches the existing `maven-guice` target).

**Decision**: Restore the `pants-example-jvm` `CorpusTarget` entry in-place — remove the comment block, add the entry structured like the other pants-example-* entries. Follow PR #757's layer 1 pattern: assert `pkg:maven/*` component count ≥ N + assert one known top-level coord present + assert `waybill:pants-resolve=<name>` annotation attached.

## R5 — Empirical top-level coordinates in the fixture

**Question**: What are the top-level Maven coordinates in `pantsbuild/example-jvm/3rdparty/jvm/default.lock` that we can use as FR-005 anchors?

**Investigation**: Read the fixture's PANTS LOCKFILE METADATA header block (already inspected during PR #757 preparation).

**Findings**:

The fixture's `generated_with_requirements` array declares 4 top-level coordinates:

1. `com.google.guava:guava:31.0.1-jre`
2. `com.lihaoyi:acyclic_2.13:0.2.1`
3. `org.scala-lang:scala-library:2.13.8`
4. `org.scalatest:scalatest_2.13:3.2.10`

The full resolved graph has 27 `[[entries]]` blocks (transitive closure of the 4 top-level coords).

**Decision**: FR-005 anchors:

1. `pkg:maven/com.google.guava/guava@31.0.1-jre` — top-level, stable, well-known
2. `pkg:maven/org.scala-lang/scala-library@2.13.8` — top-level, distinguishes Scala-flavored classifier handling
3. Count assertion: `pkg:maven/*` components ≥ 20 (of the 27 observed; leaves headroom for reader-refinement drift within safe bounds)

**Rationale**: Dual-anchor (guava + scala-library) mirrors the pants-example-django pattern (`django` + count). Count floor at 20 catches ≥ 25% drop with headroom for benign lockfile refresh.

## Summary of decisions ready for Phase 1

| Decision | Value |
|---|---|
| Option A vs Option B | **Option B** — delete `direct_dependencies` field |
| Production diff estimate | ~5-10 lines (delete field, delete dead-code sink line, verify existing fail-open covers FR-004) |
| Test updates | Delete `direct_dependencies: Vec::new()` on line 497 of lockfile.rs test fixture. No integration test changes. |
| New unit tests | 5 per FR-007: coord-table 1-dep, coord-table N-dep, mixed shapes, malformed coord warn-skip (existing behavior test), legacy string form (synthetic verify) |
| Corpus target | Restore `pants-example-jvm` entry in `manifest.rs`. New `pants_example_jvm_layer1` in `layer1_assertions.rs`. Fork + SHA unchanged from PR #757 reservation. |
| Layer 1 assertions | (1) count ≥ 20, (2) `pkg:maven/com.google.guava/guava@` present, (3) `pkg:maven/org.scala-lang/scala-library@` present, (4) `waybill:pants-resolve` annotation present on maven components |
| Zero new Cargo deps | Confirmed |
| Zero waybill-common changes | Confirmed |
