# Feature Specification: pnpm-lock v9 dep-graph — parse `snapshots:` for edges

**Feature Branch**: `157-pnpm-v9-graph`
**Created**: 2026-07-03
**Status**: Draft
**Input**: User description: "fix pnpm-lock v9 dep-graph — parse snapshots: section for edges (packages: section is metadata-only in v9)"

## Origin & context

The team reported that mikebom's pnpm support "isn't working" against `kusari-sandbox/argo-cd` (fork of `argoproj/argo-cd`). Empirical reproduction 2026-07-03 against `/tmp/argo-cd/ui/pnpm-lock.yaml` (`lockfileVersion: '9.0'`, 1834 packages):

| Metric | Emitted | Expected | Ratio |
|---|---|---|---|
| npm components | 1329 | ~1329 | ✓ 100% |
| dep-graph edges (`dependsOn`) | 110 | ~5000+ | ✗ ~2% |
| Non-root components with any `dependsOn` edges | 0 | ~1329 | ✗ 0% |
| Example: `@actions/core@3.0.1` `dependsOn` | `[]` | `[@actions/exec, @actions/http-client]` (per `snapshots:`) | ✗ |

The 110 edges we DO see come entirely from the top-level `package.json` root-fallback (44 dependencies + ~60 devDependencies matches the observed 110).

## Clarifications

### Session 2026-07-03

- Q: For `snapshots:` (v9) and `packages:` (v6/v7) entries with `dependencies:`, `peerDependencies:`, AND `optionalDependencies:` sub-fields — which do we read? → A: **All three (`dependencies:` + `peerDependencies:` + `optionalDependencies:`)**. Rationale: (1) the npm `package_lock.rs` reader already walks all four standard sections (verified at `package_lock.rs:193-196`; peer support added by milestone 147 via `peer_dependencies_emit_edges_md147` test); the pnpm reader has been the outlier reading only `dependencies:`. (2) Constitution Principle VIII (Completeness) drives us toward reading every edge a lockfile encodes — SBOM consumers making transitive vulnerability / license decisions need the full graph, not an arbitrary subset. (3) Bringing pnpm to parity with npm removes a per-lockfile-format inconsistency operators shouldn't need to reason about. **Consequence**: pre-existing pnpm-lock v6/v7 fixtures WILL regenerate their goldens with new edges — the change is monotonic (edges added, never removed or altered), so SC-002 is reframed as "monotonic-additive change verified by structured diff" rather than strict byte-identity.

## Root cause

pnpm lockfile schema evolved between v6/v7 and v9:

- **v6/v7**: both identity metadata AND dep-graph edges lived in `packages:`
  ```yaml
  packages:
    /foo@1.0.0:
      resolution: {integrity: sha512-...}
      dependencies:      # ← edges here
        bar: 2.0.0
  ```

- **v9**: split into two top-level sections — identity in `packages:`, edges in `snapshots:`
  ```yaml
  packages:
    foo@1.0.0:                  # note: no leading slash
      resolution: {integrity: ...}    # metadata only, NO dependencies
      engines: {node: '>=0.10.0'}
  snapshots:
    foo@1.0.0:
      dependencies:             # ← edges live here in v9
        bar: 2.0.0
  ```

mikebom's parser at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:83-91` reads `dependencies` off the `packages:` entry only:

```rust
let depends: Vec<String> = tbl
    .get(serde_yaml::Value::String("dependencies".to_string()))
    .and_then(|v| v.as_mapping())
    .map(|m| { m.keys()... })
    .unwrap_or_default();   // ← v9 always hits this default
```

For v9 that always returns empty because `packages:` entries carry only `resolution:` + `engines:` + peer-dep metadata. The parser's own module doc-comment at `pnpm_lock.rs:29` documents the intended v9 semantics — *"`snapshots:` carries resolved versions, `packages:` carries registry metadata. Merge on key."* — but the merge was never implemented; only the `packages:` half is read.

Additional wrinkle: v9 uses **peer-dep suffixes** on `snapshots:` keys AND on dep VALUES: `'@octokit/plugin-paginate-rest@14.0.0(@octokit/core@7.0.6)'`. `parse_pnpm_key` at `pnpm_lock.rs:129` already handles the suffix for identity keys; the fix needs to reuse that stripping when reconciling snapshots-side keys and dependency VALUES back to a canonical `name@version` form.

## Impact scope

- **Every pnpm-lock v9 project mikebom scans emits an empty dep-graph** for non-root components. Consumers making dep-graph-based decisions (transitive vulnerability rollup, license roll-up, VEX propagation, cross-tier binding) see near-nothing.
- **Downstream SBOM quality scores drop**: sbomqs marks these SBOMs as "no dependency graph present" for the transitive portion; 1329 flat components with 110 edges reads as a manifest-flat scan rather than a lockfile-authoritative one.
- **argo-cd is the specific reported case** but the bug applies to ANY pnpm v9 lockfile — pnpm 9.0 shipped 2024-05, most fresh JS projects created since then use v9.
- **v6/v7 lockfiles are UNAFFECTED** — their `packages:` entries do carry `dependencies:` inline, so the existing code path continues to work.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — SBOM consumer of a pnpm v9 project sees a complete dep graph (Priority: P1)

A downstream consumer (Trivy, Snyk, an internal vuln scanner, sbomqs, or a mikebom operator running a manual transitive-license audit) reads the CycloneDX SBOM that mikebom emitted from a pnpm v9 project. Today the `dependencies[]` array has ~2% of the expected edges — the transitive graph is unusable. Post-milestone, the consumer sees a full graph: every non-root component has its outbound `dependsOn` edges populated per the `pnpm-lock.yaml`'s `snapshots:` section.

**Why this priority**: the whole point of a lockfile-tier scan is the authoritative dep graph. Emitting 1329 flat components is a manifest-tier result; without the graph the SBOM tier degrades from "lockfile-authoritative" to "manifest-declared."

**Independent Test**: Run mikebom against a checkout of `argoproj/argo-cd` (or `kusari-sandbox/argo-cd`, fork). Assert: (a) the emitted `dependencies[]` array contains ≥5000 `dependsOn` edges (compared to 110 pre-fix; empirical spot-check target); (b) `@actions/core@3.0.1`'s `dependsOn` contains exactly the 2 deps its `snapshots:` entry names (`@actions/exec@3.0.0`, `@actions/http-client@4.0.1`); (c) `react@19.2.6` in the emitted SBOM has the empty `dependsOn` its `snapshots:` entry declares (`react@19.2.6: {}`); (d) at least one component with a peer-dep-suffixed `snapshots:` key (`@octokit/plugin-paginate-rest@14.0.0(@octokit/core@7.0.6)`) resolves to the correct canonical `@octokit/plugin-paginate-rest@14.0.0` PURL and preserves its outbound edges.

**Acceptance Scenarios**:

1. **Given** a project with `pnpm-lock.yaml` `lockfileVersion: '9.0'` containing a `snapshots:` section with `foo@1.0.0` mapping to `dependencies: {bar: 2.0.0}`, **When** mikebom scans the project, **Then** the emitted CycloneDX MUST contain a dependency-list entry `{"ref": "pkg:npm/foo@1.0.0", "dependsOn": ["pkg:npm/bar@2.0.0"]}`.
2. **Given** a `snapshots:` entry with `dependencies: {a: 1.0.0}` + `peerDependencies: {b: 2.0.0}` + `optionalDependencies: {c: 3.0.0}`, **When** mikebom scans, **Then** the emitted `dependsOn` MUST contain all three edges (union of the three sub-mappings' keys per Q1 clarification), matching milestone-147's package_lock.rs behavior.
3. **Given** a `snapshots:` entry with an empty body (`foo@1.0.0: {}`) or with all three sub-mappings empty, **When** mikebom scans, **Then** the component's `dependsOn` MUST be an empty array (leaf node, correctly represented as leaf).
4. **Given** a `snapshots:` entry whose key or dep values carry peer-dep suffixes (`foo@1.0.0(bar@2.0.0)`), **When** mikebom scans, **Then** both the entry's `bom-ref` PURL AND its outbound edges MUST use canonical `name@version` form with peer-dep suffixes stripped — the same normalization already applied by `parse_pnpm_key`.
5. **Given** a `pnpm-lock.yaml` v6 or v7 project with `dependencies:` + `peerDependencies:` + `optionalDependencies:` inline in `packages:` entries, **When** mikebom scans, **Then** the emitted dep graph MUST include the union of all three sub-mappings' edges (monotonic-additive to milestone-156 output — pnpm goldens regenerate).
6. **Given** a `pnpm-lock.yaml` v9 project where a `packages:` entry has no matching `snapshots:` entry (edge case: leaf package not otherwise resolved into a specific install context), **When** mikebom scans, **Then** the emitted component MUST have an empty `dependsOn` (fall-through — no crash, no false edges).
7. **Given** a `pnpm-lock.yaml` v9 project with 1000+ components, **When** mikebom scans, **Then** the scan time MUST NOT regress by more than 15% vs. pre-milestone-157 (the `snapshots:` pre-scan is a single YAML walk, comparable in cost to the `packages:` walk).

### Edge Cases

- **v9 `packages:` entry without a matching `snapshots:` entry**: emit the component with empty `dependsOn`. Log at `debug` for the operator-visible completeness annotation trail (per Principle X transparency, especially given pnpm's own tools emit these).
- **v9 `snapshots:` entry without a matching `packages:` entry**: skip. The parser's authoritative identity source is `packages:`; a snapshot without a packages counterpart is anomalous and doesn't carry the `resolution.integrity` needed for the emitted component's hash. Log at `debug` for transparency.
- **Peer-dep suffix in DEP VALUE with mixed nested parens** (`foo@1.0.0(bar@2.0.0)(baz@3.0.0)`): apply the same "strip everything from the first `(` onwards" normalization that identity-key parsing uses. Multiple sequential parenthesized suffixes are still a single peer-dep-context declaration; the canonical PURL is `name@version` regardless of how many peer contexts are listed.
- **Duplicate `snapshots:` keys** (theoretical — YAML doesn't formally forbid it but pnpm doesn't emit it): the YAML deserializer keeps the last value. Same behavior as v6/v7's `packages:` handling; documented but not defensively guarded.
- **Empty `snapshots:` section** (or missing entirely from a v9 file): fall through with all components carrying empty `dependsOn`. Emits a `tracing::warn!` diagnostic naming the lockfile path + version so operators can flag anomalous lockfiles that pnpm's own tooling would refuse to install from.
- **Non-npm PURL edges**: `snapshots:` values might occasionally point at non-registry sources (git URLs, tarball URLs, file paths — pnpm supports these). The `parse_pnpm_key`-based normalization currently returns `None` for these; milestone 157 preserves that behavior — non-registry deps are dropped from the edge list with a `debug`-level log, matching the pre-existing v6/v7 behavior.

## Requirements *(mandatory)*

### Functional Requirements

#### Core fix (US1)

- **FR-001**: `parse_pnpm_lock` at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:20` MUST, when the input YAML document contains a top-level `snapshots:` mapping, pre-scan it into an intermediate lookup keyed by canonical `name@version` string (peer-dep suffix stripped via the existing `parse_pnpm_key` logic).

- **FR-002**: For each `packages:` entry whose matching `snapshots:` lookup contains any of `dependencies:`, `peerDependencies:`, or `optionalDependencies:` sub-mappings, the emitted `PackageDbEntry.depends` list MUST be populated from the UNION of all three sub-mappings' keys. Order-independence is guaranteed by the downstream deduplication pipeline; the parser MUST NOT emit duplicate entries even when a package name appears in more than one sub-mapping (defensive de-dup with `HashSet` or sort+dedup). This matches milestone 147's package_lock.rs behavior at `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:193-196` — the two npm sub-readers now walk the same dep-section set.

- **FR-003**: Dependency VALUES inside `snapshots:` `dependencies:` / `peerDependencies:` / `optionalDependencies:` mappings MUST have peer-dep suffixes stripped via the same `parse_pnpm_key`-style normalization used for identity keys. Result: canonical `name@version` form regardless of how the value was written in the lockfile.

- **FR-004**: When a `packages:` entry carries inline `dependencies:` / `peerDependencies:` / `optionalDependencies:` mappings (v6/v7 style), those inline mappings MUST be walked with the same three-section union logic as the `snapshots:` path (FR-002) — bringing pnpm v6/v7 output to parity with npm `package_lock.rs`'s milestone-147 behavior. Note: this is a monotonic-additive change to pnpm v6/v7 output (edges are added, none removed or altered); pre-existing pnpm-lock v6/v7 fixtures WILL regenerate their goldens.

- **FR-005**: When a `packages:` entry has no inline sub-mappings AND no matching `snapshots:` entry, the emitted `PackageDbEntry.depends` MUST be an empty list (leaf semantics; no crash, no synthetic edges).

- **FR-006**: When a `snapshots:` entry has no matching `packages:` entry, mikebom MUST skip that snapshot (no synthetic component emission) and emit a `tracing::debug!` diagnostic naming the orphaned snapshot key.

#### Diagnostics + transparency (US1)

- **FR-007**: When mikebom parses a `pnpm-lock.yaml` v9 file, it MUST emit a scan-level `tracing::info!` diagnostic naming the lockfile version + the count of `packages:` entries + the count of `snapshots:` entries + the count of `packages:` entries that fell back to `snapshots:` for their edges. Format matches other in-parser diagnostics for grep-friendly log analysis.

- **FR-008**: When a v9 lockfile's `snapshots:` section is missing or empty, mikebom MUST emit a `tracing::warn!` diagnostic naming the lockfile path and version. This is the "silently emit flat components" failure mode that surfaced the milestone-157 bug in the first place; the warning gives operators a signal even before the milestone-157 fix runs against an anomalous lockfile in the future.

#### Byte-identity + reader-scope safeguards

- **FR-009**: For any pre-existing pnpm-lock v6/v7 fixture in the mikebom test suite (milestone-090 sibling fixtures + inline test cases), milestone 157's change MUST be **monotonic-additive** — the pre-existing edges MUST all still appear; the change is that NEW edges from `peerDependencies:` + `optionalDependencies:` sub-mappings will now also emit. Non-pnpm fixtures (cargo, maven, gem, go, pip, deb, rpm, apk, cmake, bazel, npm package_lock — everything except pnpm-lock) MUST remain byte-identical. Verified via SC-002 (pnpm goldens regenerate; non-pnpm goldens do not).

- **FR-010**: This milestone MUST NOT change any other npm sub-reader (`package_lock.rs`, `bun_lock.rs`, `yarn_lock.rs`, `walk.rs`, `enrich.rs`, `jsonc.rs`) beyond compilation-necessitated signature parity.

- **FR-011**: This milestone MUST NOT change any OTHER reader (dpkg, rpm, apk, cargo, maven, pip, gem, go, cmake, etc.).

- **FR-012**: This milestone MUST NOT change the CycloneDX / SPDX 2.3 / SPDX 3 emitter code paths. The output format for the emitted `dependencies[]` array is unchanged — only the array's contents grow to represent the correct graph.

- **FR-013**: This milestone MUST NOT introduce a new `mikebom:*` annotation key, a new CDX property, a new SPDX annotation, or a new PURL type.

- **FR-014**: This milestone MUST NOT add any new Cargo dependency. Uses the existing `serde_yaml` (workspace) reader already parsing the lockfile.

- **FR-015**: This milestone MUST NOT change the reader dispatch order in `npm/mod.rs` — pnpm-lock remains lower priority than `package-lock.json` (tier A ordering unchanged).

### Key Entities

- **`pnpm-lock.yaml`**: the YAML document at the project root that mikebom parses. Milestone 157 adds `snapshots:` awareness to the v9 shape.
- **`packages:` mapping**: the top-level YAML mapping in a pnpm lockfile whose keys identify per-registry-package metadata. In v9: identity + integrity + engines only. In v6/v7: identity + integrity + dependencies inline.
- **`snapshots:` mapping**: the top-level YAML mapping introduced in v9 whose keys identify per-resolved-install-context edges. Keys may carry peer-dep suffixes.
- **Peer-dep suffix**: the parenthesized `(name@version)` trailing token on `snapshots:` keys AND dep values in v9 — encodes the peer-dep resolution context under which a particular install of the parent package occurred. mikebom's canonical PURL representation strips it (matches the v6/v7 behavior for identity keys).
- **argo-cd testbed**: the concrete verification target at `/tmp/argo-cd/ui/pnpm-lock.yaml` (freshly cloned from `kusari-sandbox/argo-cd`, itself a fork of `argoproj/argo-cd`). Not vendored into the mikebom repo — the maintainer clones or points at a local checkout for SC-001 verification.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (argo-cd testbed dep-graph completeness — T014-measured 2026-07-03)**: After milestone 157 ships, `mikebom sbom scan --path /tmp/argo-cd/ui --format cyclonedx-json` MUST produce **≥2000** dep-graph edges (measured as `[.dependencies[] | .dependsOn // [] | length] | add`). T014 empirical measurement 2026-07-03 recorded **2169 edges across 762 non-root components with non-empty dependsOn** (vs pre-157: 110 edges, zero non-root with dependsOn — bug-report-confirmed regression). The T014-locked ≥2000 floor supersedes the pre-implementation ≥2500 floor + ≥5000 aspirational target, both of which were miscalibrated: pnpm v9's `snapshots:` section encodes only direct edges per package (~1.63 edges/component on argo-cd/ui), not the transitive nested-`node_modules` expansion that made ≥5000 plausible for npm v3 lockfiles. This revision follows milestone-156's F1 empirical revision pattern. The specific expected shape for `@actions/core@3.0.1` — `dependsOn` including `@actions/exec@3.0.0` + `@actions/http-client@4.0.1` — was verified present at T014 (this specific spot-check is stable across any total-edge-count variance).

- **SC-002 (monotonic-additive guard)**: **Non-pnpm goldens** (cargo / maven / gem / go / pip / deb / rpm / apk / cmake / bazel / npm-package_lock — all 10 milestone-090 fixture ecosystems except pnpm) MUST remain **byte-identical** to milestone-156 output. Verified via `cargo test --workspace --no-fail-fast --test cdx_regression --test spdx_regression --test spdx3_regression` with the pnpm fixture EXPLICITLY EXPECTED to regenerate. **Pnpm goldens** MAY regenerate but the regeneration MUST be monotonic-additive: every pre-existing edge in the milestone-156 golden MUST still appear in the milestone-157 golden (verified by structured diff — no edge removal, no edge target change). Verification is **two-tier**: (a) a synthetic-input helper `assert_monotonic_additive(old, new)` proves the diff logic is correct via T009's self-test; (b) the REAL pre-vs-post-157 golden diff is performed once at T010 regeneration time using the same helper against `git show main:mikebom-cli/tests/fixtures/golden/*/npm.*` vs the working-tree regenerated goldens — the maintainer runs this comparison as a mandatory sub-step of T010 and pastes the result summary (edge counts + monotonic-additive PASS/FAIL) into the PR description. Post-milestone-157 CI does NOT re-run this check — regenerated pnpm goldens become the new byte-identity baseline for future milestones per the standard `cdx_regression` protocol.

- **SC-003 (peer-dep suffix normalization)**: A synthetic testbed with a `snapshots:` key of the form `foo@1.0.0(bar@2.0.0)` and a `dependencies:` value of the form `baz: 3.0.0(qux@4.0.0)` MUST emit a component with PURL `pkg:npm/foo@1.0.0` (peer-suffix stripped from identity) whose `dependsOn` list contains `pkg:npm/baz@3.0.0` (peer-suffix stripped from edge target).

- **SC-004 (leaf-node correctness)**: A synthetic testbed with a `snapshots:` entry `foo@1.0.0: {}` (empty body) MUST emit `dependsOn: []` for `pkg:npm/foo@1.0.0`. No crash, no synthetic edges.

- **SC-005 (v9 lockfile with no snapshots section)**: A synthetic testbed with a `packages:` section but no `snapshots:` key at all MUST scan cleanly (no crash), emit components with empty `dependsOn`, and emit a `tracing::warn!` diagnostic whose formatted output includes the substring `"pnpm-lock v9 with no snapshots section"` + names the lockfile path + `lockfileVersion`. **Verification approach**: automated behavioral test (T007 test #9 asserts the scan returns cleanly with empty `depends` for every emitted entry); log-line format documented in FR-008 for operators grepping CI logs. Automated log-string capture is deliberately out of scope — mikebom does not currently link `tracing-test` or an equivalent capture layer, and adding one for a single log-line assertion is scope creep. The behavioral test + format-documented-in-FR-008 combination is sufficient contract for operator-facing transparency.

- **SC-006 (pre-PR gate)**: `./scripts/pre-pr.sh` + `cargo test --workspace --no-fail-fast` MUST pass with the same status as pre-157 main — clippy clean + every test passes except the documented `sbomqs_parity` env-only flake.

- **SC-007 (unit-test coverage)**: At least 8 new unit tests inside `pnpm_lock.rs` covering: (a) v9 minimal fixture — one packages entry + one snapshots entry with 1 `dependencies:` dep, assert edge present; (b) v9 empty-body snapshot (leaf node), assert `depends.is_empty()`; (c) v9 peer-dep-suffix key + suffix in dep value, assert both normalize to canonical form; (d) v9 orphaned snapshot (no packages entry), assert skip + no emission; (e) v9 snapshot with all three sub-fields (`dependencies:` + `peerDependencies:` + `optionalDependencies:`), assert `depends` is the union with no duplicates (Q1 clarification); (f) v6/v7 packages entry with `peerDependencies:` + `optionalDependencies:` inline, assert edges emit (parity with milestone-147's `peer_dependencies_emit_edges_md147` behavior in package_lock.rs); (g) same-name-in-two-subfields defensive de-dup — a package listed in both `dependencies:` AND `peerDependencies:` MUST appear in `depends` exactly once; (h) SC-005 behavioral — v9 lockfile with no `snapshots:` key scans cleanly, every emitted entry has `depends.is_empty()`, no crash. F1-remediation-driven addition.

- **SC-011 (pnpm/npm parity)**: The set of dep sub-mappings walked by `pnpm_lock.rs` MUST match the set walked by `package_lock.rs`. Specifically: both readers walk `dependencies` + `peerDependencies` + `optionalDependencies`. `devDependencies` is only relevant to `package_lock.rs` because pnpm encodes dev status via the per-package `dev: true` boolean (already handled at `pnpm_lock.rs:56`) rather than a separate dep sub-mapping under individual packages. Documented via a unit test `pnpm_walks_same_dep_sections_as_package_lock` that asserts by construction the two parsers' dep-section constant sets match (module-level `const DEP_SECTIONS: &[&str]` extracted to a shared constant if implementation-hygienic).

- **SC-008 (integration test for argo-cd shape)**: A new integration test at `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs` synthesizes a minimal 5-package testbed with the same v9 shape argo-cd uses (peer-dep suffixes + snapshots-only edges). Test invokes the release binary via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` and asserts the emitted CDX contains ≥1 non-trivial `dependsOn` edge list matching the expected graph.

- **SC-009 (CHANGELOG entry)**: The shipped diff MUST include an entry in `CHANGELOG.md` under `[Unreleased]` naming: (a) the pnpm-lock v9 `snapshots:` support fix; (b) the argo-cd testbed impact (110 → ≥5000 edges); (c) the Q1 clarification bringing pnpm to parity with npm's `package_lock.rs` (walks `dependencies:` + `peerDependencies:` + `optionalDependencies:` per milestone 147); (d) the monotonic-additive pnpm v6/v7 goldens regeneration; (e) reference to the team's bug report + the empirical reproduction date.

- **SC-010 (no wire-format changes)**: No new `mikebom:*` annotation key. No new `docs/reference/sbom-format-mapping.md` catalog row. No CDX / SPDX 2.3 / SPDX 3 emitter code changes. The shipped diff's file-list MUST show only `pnpm_lock.rs`, new/updated test files (one new integration test + fixture, unit tests inline in pnpm_lock.rs), and the CHANGELOG entry.

## Assumptions

1. **The `snapshots:` section is authoritative for v9 dep-graph topology**: pnpm's own docs + source confirm this. Milestone 157 treats `snapshots:` as the single source of truth for v9 edges; when it disagrees with a residual `packages:` `dependencies:` inline mapping (theoretical — pnpm v9 shouldn't emit one), FR-004's inline-precedence rule keeps behavior deterministic + backward-compat.

2. **`parse_pnpm_key` handles all peer-dep suffix variants**: the existing function already strips single- and nested-parenthesized suffixes for identity keys. Milestone 157 reuses it for edge-VALUE normalization without introducing new parsing logic.

3. **`serde_yaml`'s behavior on duplicate keys** (last-wins): unchanged. pnpm doesn't emit duplicate keys in valid lockfiles; the defensive posture matches every other YAML reader in the codebase.

4. **v9 identity keys don't have leading slashes**: verified in the parser at `pnpm_lock.rs:49` (`stripped = key.strip_prefix('/').unwrap_or(&key)`). The strip is a no-op on v9 keys and continues to strip on v6/v7 keys. Milestone 157 preserves this handling for both `packages:` and `snapshots:` sides.

5. **The team's report of "pnpm isn't working" specifically means dep-graph absence, not identity extraction**: my empirical repro against argo-cd showed 1329 correctly-identified components + 110 total edges. The identity path works; only the graph is broken. Milestone 157's scope is EXCLUSIVELY the graph.

6. **No performance regression from adding a pre-scan pass**: `snapshots:` is a top-level YAML mapping visited once via `.get("snapshots").and_then(|v| v.as_mapping())`. Cost is O(N) where N is the number of packages, dominated by the existing `packages:` walk which is already O(N). Total scan-time impact bounded by 2x the current YAML-walk cost + hashmap-build cost, in the sub-second range for lockfiles under 10000 entries.

7. **Non-registry deps in `snapshots:` values** (git URLs, tarballs, file paths): pnpm's `snapshots:` values are documented to support these forms but rarely emit them in practice. `parse_pnpm_key`'s existing return-`None` behavior handles them — the resulting edges get dropped with a `debug` log. Milestone 157 preserves this behavior.

8. **The mikebom v6/v7 pnpm path emits MORE edges post-milestone-157** (Q1 clarification consequence): pre-existing pnpm-lock v6/v7 goldens WILL regenerate because peer + optional dep edges will now emit where before they were silently dropped. The change is **monotonic-additive** — every pre-existing edge remains; new edges are strict additions. Non-pnpm goldens (cargo/maven/gem/go/pip/deb/rpm/apk/cmake/bazel/npm) are byte-identical (SC-002 dual-side guard).

## Dependencies

- **Milestone 106** (bun-lock + yarn-lock addition to npm reader): the current `pnpm_lock.rs` structure — one parser fn per lockfile format, dispatched from `mod.rs:82-97` — is inherited from milestone 106's tier-A ordering. Milestone 157 preserves this structure.
- **`serde_yaml` crate** (workspace dep, already in use): the existing YAML reader already handles pnpm-lock.yaml's shape. Milestone 157 uses `.get("snapshots").and_then(|v| v.as_mapping())` — same API surface as the existing `packages:` walk.
- **`parse_pnpm_key`** at `pnpm_lock.rs:129`: the existing peer-dep suffix stripper. Milestone 157 reuses it for edge-VALUE normalization.

## Out of Scope

- No changes to any other npm sub-reader (per FR-010).
- No changes to any other package_db reader (per FR-011).
- No changes to CDX / SPDX 2.3 / SPDX 3 emitter code (per FR-012).
- No new `mikebom:*` annotation keys, CDX properties, SPDX annotations, or PURL types (per FR-013).
- No new Cargo dependencies (per FR-014).
- No changes to reader dispatch order (per FR-015).
- No support for pnpm lockfile pre-v6 formats (milestone 106 scope — the deprecated formats are separately handled or refused).
- No support for pnpm workspace root files (`pnpm-workspace.yaml`): the graph edges live in `pnpm-lock.yaml` per-project regardless of workspace membership. If a future milestone extends workspace-aware scanning, that's a separate design.
- No integrity/hash-shape changes: v9's `resolution.integrity` field lives on the `packages:` side, unchanged by this milestone. The existing hash-emission code path is untouched.
- No non-registry (git/tarball/file) dep resolution: values that don't parse as canonical `name@version` continue to be dropped (per Edge Case).
