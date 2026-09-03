# Research — Pants JavaScript/npm corpus regression gate

## R1 — Upstream fixture survey

**Question**: Does a small, stable, public Pants-managed JavaScript monorepo exist that we can pin as the corpus target?

**Investigation**: Queried the GitHub API for `pantsbuild/example-*` variants matching JS naming conventions (`example-nodejs`, `example-node`, `example-javascript`, `example-js`, `example-typescript`). Also considered community OSS projects using Pants for JS as a fallback.

**Findings**:

- `pantsbuild/example-javascript` exists and is production-appropriate:
  - **HEAD SHA (as of 2026-09-02)**: `da76d5dbb407d82c136cfe8f18dc06f3c8a440e5`
  - **Size**: 95 KB (fastest clone in the pants example corpus, one order of magnitude smaller than example-django)
  - **Push cadence**: last pushed 2026-02-21 (7 months prior to survey; stable, not abandoned)
  - **License**: Apache-2.0 (matches other pantsbuild examples; matches PR #757 fork policy)
  - **Package manager**: `npm` (declared in `pants.toml` via `[nodejs] package_manager = "npm"`) — matches MVP scope Q3 clarification
  - **Lockfile**: `package-lock.json` at repo root, `lockfileVersion: 2`, 316 total packages (315 non-root)
  - **Direct deps**: none (in `package.json.dependencies`)
  - **Direct devDeps**: `esbuild@^0.20.1` and `jest@^29.7.0` (in `package.json.devDependencies` — mirrored in `package-lock.json` `packages[""].devDependencies`)
  - **Pants BUILD targets used**: `package_json`, `node_build_script`, `node_test_script`, `javascript_sources`, `javascript_tests` — exercises the Pants-JS BUILD-target vocabulary
  - **Pants config**: `[nodejs]` section with `package_manager = "npm"` + `backend_packages` enabling `pants.backend.experimental.javascript` + `pants.backend.experimental.javascript.lint.prettier`
- Other `pantsbuild/example-*` JS-adjacent names (`example-nodejs`, `example-node`, `example-js`, `example-typescript`) return 404. `example-javascript` is the only official pantsbuild JS example.

**Decision**: Fork `pantsbuild/example-javascript` at SHA `da76d5dbb407d82c136cfe8f18dc06f3c8a440e5` into `kusari-sandbox/example-javascript`. This matches the pattern PR #757 established for pants-example-{python,django,golang}.

**Rationale**: Public-monorepo path (FR-002 option A) wins. Synthetic fallback (FR-002 option B) is unneeded — the pants ecosystem's own official example directly satisfies our requirements. Fixture-selection risk retired.

**Alternatives considered**:

- **Community OSS Pants-JS monorepo**: skipped once the pantsbuild official example was located. A pantsbuild-owned fixture has the strongest signal for "official upstream shape" — a community project would introduce fixture-quality variance without additional signal.
- **Synthetic fixture in waybill-test-fixtures**: fallback path per FR-002; not needed here. Retained as a documented option in the spec so if `example-javascript` is later archived or replaced, the fallback path is already spec-blessed without a spec revision.

## R2 — Empirical smoke scan against the pinned fixture

**Question**: What does waybill emit today when scanning `pantsbuild/example-javascript` at the pinned SHA? Does the output satisfy the layer 1 assertion invariants the spec calls for?

**Investigation**: Cloned the pinned SHA into a scratch dir, ran `waybill --offline sbom scan --format cyclonedx-json --root-name pants-example-javascript --root-version da76d5d --path .`, and inspected the emitted CDX.

**Findings**:

| Metric | Observed |
|---|---|
| Total components emitted | **303** |
| `pkg:npm/*` components | **302** |
| Non-npm components | 1 (root-override placeholder from `--root-name` + `--root-version`) |
| `waybill:graph-completeness` | `complete` (302/302 reachable, 0 orphans, 0 reason-codes) |
| Full SBOM byte size (CDX only) | ~570 KB |
| Full SBOM component-graph relationships | 640 |
| `pkg:npm/esbuild@*` present | ✅ yes (top-level devDep) |
| `pkg:npm/jest@*` present | ✅ yes (top-level devDep) |
| `waybill:pants-resolve` annotation on npm components | ❌ absent (as expected per FR-006 — Pants-side provenance annotations are not attached today) |
| `waybill:pants-target` annotation on npm components | ❌ absent (as expected per FR-006) |

**Decision**: Layer 1 assertions will check:

1. `pkg:npm/*` component count ≥ 250 (below the observed 302, above any plausible 10% regression threshold per SC-006).
2. `pkg:npm/esbuild@*` present (top-level devDep anchor per FR-005).
3. `pkg:npm/jest@*` present (top-level devDep anchor — dual anchor guards against regressions that break exactly one direct-dep resolution).
4. Zero `waybill:pants-resolve` or `waybill:pants-target` annotations on any `pkg:npm/*` component (FR-006 regression-lock — if a future change starts emitting Pants-side annotations on npm surface, the assertion fires and the maintainer decides whether to update the goldens or file a spec revision).

**Rationale**: 302 is a healthy signal-density floor. A regression that silently drops even 10% (down to ~271) still passes assertion 1's ≥ 250 threshold — so we chose 250 to be tighter than SC-006's stated 10% threshold, giving a per-target signal that catches smaller regressions than the spec-level goal. Assertions 2 + 3 catch direct-dep resolution regressions; assertion 4 catches accidental Pants-side annotation leakage.

**Alternatives considered**:

- **Threshold at exactly 302 (byte-identical count)**: rejected — brittle to any legitimate lockfile refresh; the fixture's lockfile could gain 1-2 transitive deps from a Jest update without the fixture being "broken", and layer 2 golden regen would already surface that as a deliberate change.
- **Threshold at 100 (matches SC-006's 10% floor loosely)**: rejected — the 10% floor in SC-006 is a spec-level minimum-signal guarantee; individual targets should be tighter when we can afford to be. 250 is the negotiated middle.

## R3 — JS-only golden filter design

**Question**: The spec (FR-008) requires layer 2 goldens to be filtered to the JS surface only. How do we implement the filter such that it's deterministic, format-agnostic, and preserves regression-signal integrity?

**Investigation**: Read the existing `waybill-cli/tests/corpus_harness_195/layer2_golden.rs` masking pass. The current pattern is `walk_mask(&mut cloned)` — a recursive `serde_json::Value` walker that structurally normalizes non-deterministic fields (timestamps, doc-IDs, hashes-in-annotations). We need to compose a JS-only filter with this same walker pattern.

**Findings**:

The three SBOM formats each have distinct component + relationship storage shapes:

- **CDX (CycloneDX 1.6 JSON)**: `.components[]` (array of objects with `.purl` field) + `.dependencies[]` (array of `{ref, dependsOn: [...]}`). Filter: keep `.components[]` entries where `.purl` starts with `pkg:npm/`. Keep `.dependencies[]` entries where `.ref` matches a kept component AND filter `.dependsOn` to only edges pointing at kept components. Preserve `.metadata` (doc-scope; already masked). Preserve top-level envelope fields (`bomFormat`, `specVersion`, `serialNumber`, `version`).
- **SPDX 2.3 JSON**: `.packages[]` (array with `.SPDXID` + `.externalRefs[].referenceLocator` for PURL) + `.relationships[]` (array of `{spdxElementId, relatedSpdxElement, relationshipType}`). Filter: keep `.packages[]` where any `externalRefs[]` PURL starts with `pkg:npm/`. Keep `.relationships[]` where both endpoints reference kept SPDXIDs. Preserve `.creationInfo`, `.documentDescribes`, top-level envelope.
- **SPDX 3.0.1 JSON-LD**: `@graph[]` is a mixed array of node types keyed by `type`. Component nodes carry `packageName` + PURL. Relationship nodes carry `from`/`to`/`relationshipType`. Filter: keep component nodes whose associated PURL starts with `pkg:npm/`. Keep relationship nodes where both endpoints reference kept elements. Preserve doc-scope nodes (`CreationInfo`, `SpdxDocument`).

**Decision**: Implement three format-specific filter functions in a new `waybill-cli/tests/corpus_harness_195/js_filter.rs` module:

- `filter_cdx_to_js(&mut serde_json::Value)`
- `filter_spdx23_to_js(&mut serde_json::Value)`
- `filter_spdx3_to_js(&mut serde_json::Value)`

Invoke each in sequence with the existing masking pass, immediately BEFORE `serde_json::to_vec_pretty()` in `layer2_golden.rs::compare_golden`. Filter is applied ONLY for the pants-example-javascript target (dispatched by target name), NOT for the other 6 corpus targets — keeps existing target byte-identity guarantees intact.

**Rationale**: A per-target filter dispatch keeps the change scoped. Layer 2 for other targets stays byte-identical to today. Format-specific filter functions (rather than a single "walk and drop everything not JS-adjacent" pass) let us handle the three format shapes precisely — the SPDX 3 JSON-LD graph flattening in particular resists generic filtering.

**Alternatives considered**:

- **Universal PURL-scheme filter across all targets**: rejected — would silently change goldens for the 6 existing targets (`pants-example-python` etc.) without a spec change. Scope creep.
- **New filter macro applied via layer 2 helper flag**: rejected — over-engineered for a single target; can revisit if we add pnpm/yarn targets that also need JS filtering (Q3 clarification defers those).
- **Filter in the assertion function (layer 1) rather than the golden pass (layer 2)**: rejected — layer 1 assertions inspect the emitted SBOM directly. Golden filtering must be at the golden-compare step, not the assertion step.

## R4 — Naming + directory conventions

**Question**: What does the new target's `CorpusTarget.name` field and its golden-fixture directory get called?

**Decision**: `pants-example-javascript` — consistent with the PR #757 pattern (`pants-example-python`, `pants-example-django`, `pants-example-golang`). Golden dir: `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/`.

**Rationale**: Zero-cognitive-load consistency with the existing 3 pants corpus targets. Anyone who has grepped for `pants-example-*` in the codebase will find this target immediately.

**Alternatives considered**:

- `pants-example-npm` (calls out the package manager instead of the language): rejected — inconsistent with the language-not-manager naming for python/django (Python uses pip/pex; naming is by language). Consistency wins.
- `pants-nodejs` (matches Pants's own `[nodejs]` config section name): rejected — the fixture repo's own name is `example-javascript`, not `example-nodejs`. Match upstream.

## R5 — Fork policy (repeat of PR #757 pattern, confirmed applicable)

**Question**: Any deviations from the PR #757 fork policy for this target?

**Decision**: None. Fork `pantsbuild/example-javascript` → `kusari-sandbox/example-javascript` at SHA `da76d5d`, same public-only-audit exception the `public_only_audit` gate already permits (relaxed in PR #757 to allow `github.com/kusari-sandbox/*`). Refresh via `scripts/corpus/refresh-pins.sh` diff-and-review.

**Rationale**: Consistency + insurance-against-upstream-force-push, verbatim reasoning from PR #757.

## Summary of decisions ready to consume in Phase 1

| Decision | Value |
|---|---|
| Fixture source | `pantsbuild/example-javascript` (fork into `kusari-sandbox/example-javascript`) |
| Pinned SHA | `da76d5dbb407d82c136cfe8f18dc06f3c8a440e5` |
| Ecosystem tag (`Ecosystem::` variant) | `Npm` |
| Target name | `pants-example-javascript` |
| Layer 1 assertions | (1) `pkg:npm/*` count ≥ 250, (2) `pkg:npm/esbuild@*` present, (3) `pkg:npm/jest@*` present, (4) no `waybill:pants-*` on any `pkg:npm/*` component |
| Layer 2 filter | Per-target JS-only filter dispatched by target name in new `js_filter.rs` module |
| Golden dir | `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/` |
| Expected golden size | Well under SC-004's 500 KB budget after JS-only filter (rough estimate: 100-200 KB total across all 3 formats) |
| No new Cargo deps | Confirmed (only `serde_json` + `regex` + `tempfile`, all existing) |
| No production waybill changes | Confirmed |
