# Quickstart — milestone 071 cross-format annotation parity

Three operator-visible recipes. Each is runnable end-to-end against the post-fix build with no special setup.

## Recipe 1 — Verify cross-format parity locally before opening a PR

The new pre-PR gate test runs automatically as part of `cargo test --workspace`, which `./scripts/pre-pr.sh` invokes. To run it directly:

```bash
cargo +stable test -p mikebom --test parity_completeness
```

Expected pass output:

```text
running 1 test
test parity_completeness_27_fixtures ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

If the test fails, the message names exactly which key + which format(s) are misaligned. Example failure (from a synthetic regression — see Recipe 3):

```text
parity_completeness: SymmetricEqual row violated

  row_id:        C18
  key:           mikebom:source-files
  cdx_count:     342
  spdx23_count:  0
  spdx3_count:   342
  hint:          SPDX 2.3 emitter is missing this key. Check
                 mikebom-cli/src/generate/spdx/annotations.rs for the
                 emit guard on `c.source_files`.
```

## Recipe 2 — Add a new annotation key in a future milestone

Suppose milestone 080 introduces `mikebom:trace-step-sequence` (an ordered array of build trace steps per component). The author makes four coordinated edits in one PR:

1. **Emit in CDX**: push a `properties` entry on the component in `generate/cyclonedx/builder.rs`.
2. **Emit in SPDX 2.3**: push through `MikebomAnnotationCommentV1` in `generate/spdx/annotations.rs` (the existing envelope handles arbitrary JSON values).
3. **Emit in SPDX 3**: push through the `Annotation.statement` path in `generate/spdx/v3_annotations.rs`.
4. **Add catalog row** to `mikebom-cli/src/parity/extractors/mod.rs`:

   ```rust
   ParityExtractor {
       row_id: "C57",
       label: "mikebom:trace-step-sequence",
       cdx: c57_cdx,
       spdx23: c57_spdx23,
       spdx3: c57_spdx3,
       directional: Directionality::SymmetricEqual,
       order_sensitive: true,  // step order is semantic
   },
   // SymmetricEqual + order_sensitive: build trace steps are an ordered sequence;
   // sort-canonicalization would discard the "what happened first" semantics.
   ```

5. **(If `order_sensitive == true` or `directional != SymmetricEqual`)** Add a row to `docs/reference/sbom-format-mapping.md` under "Cross-format annotation parity catalog" with the same one-line rationale.

Run `./scripts/pre-pr.sh` — it passes. Skip step 4 — `parity_completeness` fails with the C-4 hard-fail message naming `mikebom:trace-step-sequence` as uncatalogued.

## Recipe 3 — Verify the alpha.13 → post-fix improvement

The success criterion (SC-001) is a ≥95% reduction in component-level CFI count from the external conformance harness. To reproduce the measurement:

```bash
# Generate post-fix SBOMs for the 36-fixture suite (existing fixtures only;
# don't add new ones or you bias the comparison)
for fixture in mikebom-cli/tests/fixtures/golden/cyclonedx/*.cdx.json; do
  basename=$(basename "$fixture" .cdx.json)
  echo "$basename: produced from existing scan goldens — no regen needed"
done

# Run the external conformance harness over the existing 36 fixtures.
# This step is harness-dependent; the harness lives outside mikebom and
# emits a CFI count by ecosystem and key.
external-conformance-harness \
    --tool mikebom \
    --fixture-suite ~/path/to/36-fixture-suite \
    --report cfi-by-key

# Compare component-level CFI counts to the alpha.13 baseline:
#   - alpha.13: 11,130 component-level CFI
#   - target:   ≤ 556 (≥95% reduction)
#   - SC-002:   total findings (all kinds) ≤ 1,800 (≥85% reduction from 12,165)
```

The harness output will show the residual CFI rows. Each row should be either:

- A non-`SymmetricEqual` catalog row whose `Directionality` the harness can be configured to filter (e.g., C42 lifecycle-scope), OR
- A genuine bug to file as a follow-up.

## Recipe 4 — Synthetic drift regression test

The US4 acceptance test exists at `mikebom-cli/tests/parity_synthetic_drift.rs`. It constructs a synthetic SBOM where `mikebom:foo-experimental` is emitted only in the CDX output, then invokes the parity check programmatically and asserts the failure:

```rust
#[test]
fn synthetic_cdx_only_drift_is_rejected() {
    let cdx = synthesize_cdx_with_extra_property("mikebom:foo-experimental", "demo-value");
    let spdx23 = synthesize_minimal_spdx23();
    let spdx3 = synthesize_minimal_spdx3();

    let result = run_parity_completeness_check(&cdx, &spdx23, &spdx3);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("uncatalogued mikebom:* key"));
    assert!(err.contains("mikebom:foo-experimental"));
    assert!(err.contains("emitted-by:    [cdx]"));
}
```

This test is NOT optional — it is the proof that the C-4 hard-fail behavior works. It runs on every `cargo test --workspace`.

## Recipe 5 — Operator filtering published asymmetries (consumer-side)

External conformance harnesses can read the published parity catalog from `docs/reference/sbom-format-mapping.md` (or, for richer access, from the parity catalog JSON endpoint exposed by `mikebom parity-check --emit-catalog`) to learn which CFI rows are intentional.

Example operator workflow against an external SBOM-parity tool:

```bash
mikebom parity-check --emit-catalog --format json > /tmp/mikebom-parity.json

external-conformance-harness \
    --tool mikebom \
    --fixture-suite ~/fixtures \
    --filter-asymmetries-from /tmp/mikebom-parity.json
```

After filtering, the harness's CFI count reflects only true defects, not catalogued asymmetries. This is the recommended way to integrate mikebom-emitted SBOMs with external conformance tooling — the alternative (treat every CFI as a defect) over-reports by exactly the count of legitimate non-symmetric rows.
