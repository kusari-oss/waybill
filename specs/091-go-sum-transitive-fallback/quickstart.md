# Quickstart — milestone 091 maintainer recipes

Five maintainer-facing recipes for reproducing the pre-091 baseline, applying the step-5 fix, regenerating the milestone-083 baseline, and confirming the post-091 wall-time + per-format scope.

## Recipe 1 — Reproduce the pre-091 31-edge baseline

```bash
# Build alpha.27 mikebom (latest pre-091 release):
cargo +stable build --release -p mikebom

# Scan the cri-tools transitive-parity fixture with --offline (CI configuration):
target/release/mikebom --offline sbom scan \
    --path "$MIKEBOM_FIXTURES_DIR/transitive_parity/go" \
    --format spdx-2.3-json \
    --output /tmp/pre-091.spdx.json \
    --no-deep-hash

# Count emitted DEPENDS_ON edges:
jq '[.relationships[]? | select(.relationshipType == "DEPENDS_ON")] | length' /tmp/pre-091.spdx.json
# Expected: 31 (the alpha.27 baseline).
```

## Recipe 2 — Apply the step-5 fix

Implementation lives in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`:

```rust
// 1. Add ResolutionStep::GoSumFallback variant (line ~64).
pub enum ResolutionStep {
    GoModGraph,
    GoModCache,
    Proxy,
    GoSumFallback,  // NEW
    None,
}

// 2. Add gosum_fallback_count to LadderSummary (line ~154).
pub struct LadderSummary {
    pub graph_count: usize,
    pub cache_count: usize,
    pub proxy_count: usize,
    pub gosum_fallback_count: usize,  // NEW
    pub missing_count: usize,
}

// 3. Insert step 5 in GraphResolver::resolve (line ~322), BEFORE
//    step4_empty_fallthrough.
self.step5_go_sum_fallback(&mut map, ctx);
self.step4_empty_fallthrough(&mut map, ctx);  // unchanged; runs after step 5

// 4. Add the step5_go_sum_fallback method body per data-model §step5.
fn step5_go_sum_fallback(&self, map: &mut ModuleGraphMap, ctx: &WorkspaceContext) {
    // For each go.sum module not already in map, emit empty-edge entry.
    for module in &ctx.go_sum_modules {
        if map.contains(module) {
            continue;
        }
        map.insert(ModuleGraphEntry {
            module: module.clone(),
            requires: Vec::new(),
            source: ResolutionStep::GoSumFallback,
        });
        map.summary_mut().gosum_fallback_count += 1;
    }
    // Augment root's edge set with full closure.
    let root_id = ctx.root_module_id();  // may need helper-method addition
    map.insert(ModuleGraphEntry {
        module: root_id,
        requires: ctx.go_sum_modules.iter().cloned().collect(),
        source: ResolutionStep::GoSumFallback,
    });
}

// 5. Update LadderSummary's Display impl + tracing::info! line at
//    line ~368 to include gosum_fallback_count.
```

Update the per-format emission code at `mikebom-cli/src/generate/{cyclonedx_v1_6,spdx_2_3,spdx_3_0_1}.rs` to recognize `ResolutionStep::GoSumFallback` and emit the discriminator per contracts/go-sum-fallback.md "Per-component provenance contract" section. The existing milestone-084 emission code path handles the field structure; add `go-sum-fallback` to the value enum.

## Recipe 3 — Verify post-091 ≥130 edges

```bash
cargo +stable build --release -p mikebom

target/release/mikebom --offline sbom scan \
    --path "$MIKEBOM_FIXTURES_DIR/transitive_parity/go" \
    --format spdx-2.3-json \
    --output /tmp/post-091.spdx.json \
    --no-deep-hash

# Same query as Recipe 1:
jq '[.relationships[]? | select(.relationshipType == "DEPENDS_ON")] | length' /tmp/post-091.spdx.json
# Expected: ≥130 (up from 31).

# Spot-check one go-sum-only transitive:
jq '.packages[] | select(.name == "github.com/<some-known-transitive>") | .annotations'
# Expected: an annotation with comment containing "mikebom:resolver-step=go-sum-fallback".
```

## Recipe 4 — Regenerate the milestone-083 baseline

```bash
# Step 1: run the existing transitive_parity_go test post-fix.
cargo +stable test -p mikebom --test transitive_parity_go
# Expected: failure with `left: <new>, right: 31` — the standard count-drift signal.

# Step 2: update mikebom-cli/tests/transitive_parity_go.rs:
#   - Bump EXPECTED_MIKEBOM_EDGE_COUNT from 31 to the new count (≥130).
#   - Add at least one new EXPECTED_REPRESENTATIVE_EDGES entry that
#     exercises step 5: a (root → go-sum-only-transitive) edge whose
#     target wasn't reachable pre-091. Pick from the SBOM's emitted
#     edges:
#     jq -r '.relationships[]? | select(.relationshipType == "DEPENDS_ON") |
#            "\(.spdxElementId) -> \(.relatedSpdxElement)"' /tmp/post-091.spdx.json | head -20
#     Then resolve the SPDXIDs to PURLs and pin one as a representative.
#   - Update the doc-comment to add a "Closed by milestone 091" subsection
#     mirroring the milestone-087/088 pattern.

# Step 3: re-run, confirm pass.
cargo +stable test -p mikebom --test transitive_parity_go
# Expected: 4/4 tests pass.

# Step 4: update specs/083-transitive-correctness/research.md §8 — Ecosystem: Go
# audit row to mark the gap closed (mirror the milestone-087/088 pattern).
```

## Recipe 5 — Confirm per-format scope

```bash
# Regen goldens (only golang fixtures should regenerate):
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression

git status --short mikebom-cli/tests/fixtures/golden/
# Expected: at most 3 modified files (cyclonedx/golang.cdx.json,
#   spdx-2.3/golang.spdx.json, spdx-3/golang.spdx3.json), or empty if the
#   golang/simple-module fixture doesn't trigger step 5 (depends on whether
#   its tiny go.sum populates ctx.go_sum_modules).

# Audit the diff scope: only the per-component discriminator should change.
git diff mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json | grep -E "^[+-]" | head -20
# Expected: pairs of confidence 0.85 → 0.50 + new mikebom:resolver-step properties.
# NO PURL changes, NO component count changes, NO dep-edge endpoint changes.
```

## Recipe 6 — Final pre-PR gate

```bash
./scripts/pre-pr.sh
```

Expected: zero clippy warnings, every test suite reports `0 failed`. Standard CLAUDE.md mandatory gate.

## When in doubt

- **Step 5 emits edges but the per-component annotation doesn't show**: the per-format emission code (`generate/{cyclonedx_v1_6,spdx_2_3,spdx_3_0_1}.rs`) hasn't been updated to recognize the new `GoSumFallback` variant. Grep for the existing match arms on `ResolutionStep` and add the new case.
- **Cache-populated path now emits step-5 edges**: step 5's "if map.contains(module) { continue; }" guard is broken. Check that step 1/2/3 entries are landing in the map BEFORE step 5 runs.
- **Goldens regenerate for non-golang ecosystems**: scope creep. The change should ONLY touch golang. Investigate whether a refactor accidentally changed cross-ecosystem code.
- **Edge count rises but is < 130**: `parse_go_sum` may be filtering some entries (e.g., dropping `+incompatible` or local replace targets). Inspect `parse_go_sum`'s test cases at `legacy.rs::tests` to confirm coverage.
- **Edge count rises above trivy's 142**: we're emitting more than trivy. Most likely `<mod>@<v>` and `<mod>@<v>/go.mod` lines aren't being deduped at the (module, version) key level. Verify `parse_go_sum` returns deduped entries OR add a dedup pass before emitting.
