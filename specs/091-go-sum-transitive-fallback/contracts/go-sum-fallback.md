# Contract — milestone 091 go.sum-fallback step 5

The milestone's two contracts: (1) the new ladder step's behavior, and (2) the per-component provenance discriminator across CDX/SPDX 2.3/SPDX 3.

## CLI surface

**No new operator-facing CLI flags.** This is an internal correctness fix to `mikebom sbom scan` for Go projects in the offline+cache-empty configuration.

## Library surface (`mikebom-cli` crate)

**No new public Rust API.** All changes are internal to `mikebom-cli/src/scan_fs/package_db/golang/`:
- `ResolutionStep::GoSumFallback` variant added to the existing public enum.
- `LadderSummary.gosum_fallback_count` field added to the existing public struct.
- `step5_go_sum_fallback` method added as a private impl on `GraphResolver`.

## Step 5 algorithm contract

```text
INPUT:  ctx (WorkspaceContext) with ctx.go_sum_modules: &[ModuleId]
        map (mut ModuleGraphMap), partially populated by steps 1–3
OUTPUT: map augmented with:
        - For each module M in ctx.go_sum_modules NOT already in map:
            insert ModuleGraphEntry { module: M, requires: vec![], source: GoSumFallback }
            map.summary_mut().gosum_fallback_count += 1
        - The root module's entry (synthetic):
            insert ModuleGraphEntry {
                module: ModuleId::new(ctx.root_module_path, ctx.root_version_or_empty),
                requires: ctx.go_sum_modules.iter().cloned().collect(),
                source: GoSumFallback
            }
            (only if no step-1/2/3 entry already exists for the root module-id)
PRECONDITION:  steps 1, 2, 3 have run and populated map for any modules
                they could resolve.
POSTCONDITION: every module in ctx.go_sum_modules is in map (either via
               higher-fidelity steps or via step-5 entries).
               root module's edge set covers the full go.sum closure.
```

This contract is enforced by VR-091-006 + VR-091-007 + VR-091-008 + VR-091-009.

## Per-component provenance contract

For each `ModuleGraphEntry` with `source == ResolutionStep::GoSumFallback`, the per-format emission code MUST attach a discriminator using the milestone-084 `mikebom:resolver-step` carrier:

### CDX 1.6
```json
{
  "type": "library",
  "purl": "pkg:golang/<module>@<version>",
  "evidence": {
    "identity": [
      {
        "field": "purl",
        "confidence": 0.50,
        "methods": [
          { "technique": "manifest-analysis", "confidence": 0.50 }
        ]
      }
    ]
  },
  "properties": [
    { "name": "mikebom:resolver-step", "value": "go-sum-fallback" }
  ]
}
```

(The `confidence: 0.50` is the per-format discriminator within CDX's native vocabulary; the `mikebom:resolver-step` property carries the explicit step name for cross-format consistency.)

### SPDX 2.3
```json
{
  "SPDXID": "SPDXRef-...",
  "annotations": [
    {
      "annotator": "Tool: mikebom-...",
      "annotationDate": "...",
      "annotationType": "OTHER",
      "comment": "mikebom:resolver-step=go-sum-fallback"
    }
  ]
}
```

### SPDX 3
```json
{
  "type": "Annotation",
  "subject": "<root-software_Package>",
  "statement": "mikebom:resolver-step=go-sum-fallback"
}
```

This contract is enforced by VR-091-010 + VR-091-011 + VR-091-012.

## Per-format scope contract

| Format | Affected? | Verification |
|---|---|---|
| **CDX 1.6 golang** | YES — components reached via step 5 emit `confidence: 0.50` + `mikebom:resolver-step=go-sum-fallback` property | `golang.cdx.json` golden MAY regenerate IF the `golang/simple-module` fixture's modules now flow through step 5 |
| **SPDX 2.3 golang** | YES — annotations[] gain `mikebom:resolver-step=go-sum-fallback` entries | `golang.spdx.json` golden MAY regenerate per same condition |
| **SPDX 3 golang** | YES — Annotation elements gain the same statement | `golang.spdx3.json` golden MAY regenerate per same condition |
| **Other ecosystems' goldens** | NO — only Go reader's resolver changes | All non-golang goldens byte-identical |

If goldens regenerate, the diff scope MUST be limited to the per-component provenance discriminator + (for CDX) the confidence numeric. PURL strings, component counts, and dep-edge endpoints stay byte-identical.

## Test invocation contract

```bash
# Confirm the build compiles cleanly with the new variant:
cargo +stable build --workspace
# Expected: success.

# Confirm milestone-055 cache-populated tests pass:
cargo +stable test -p mikebom --test scan_go
# Expected: every test reports `0 failed`. NO regression in cache-populated path.

# Confirm step-5 + transitive-parity test pass:
cargo +stable test -p mikebom --test transitive_parity_go
# Expected: edge count ≥130 (up from 31); 4 representative edges pass.

# Smoke-test against the cri-tools fixture in offline-cache-empty mode:
target/release/mikebom --offline sbom scan \
    --path "$MIKEBOM_FIXTURES_DIR/transitive_parity/go" \
    --format cyclonedx-json \
    --output /tmp/post-091.cdx.json \
    --no-deep-hash
jq '[.dependencies[]?.dependsOn[]?] | length' /tmp/post-091.cdx.json
# Expected: ≥130 (up from 31 pre-091).

# Goldens: only golang fixtures may regenerate; all others byte-identical:
git status --short mikebom-cli/tests/fixtures/golden/
# Expected: at most 3 modified files (cyclonedx/golang.cdx.json, spdx-2.3/golang.spdx.json, spdx-3/golang.spdx3.json), or empty.

# Pre-PR gate:
./scripts/pre-pr.sh
# Expected: zero clippy warnings, every test suite reports `0 failed`.
```

## Performance contract

- Step 5 wall-time: ≤10 ms on a 262-line `go.sum` (the cri-tools fixture). One HashMap insert per (module, version) pair.
- Total scan wall-time: within ±5% of pre-091 baseline. Step 5 adds work only in offline-cache-empty mode where steps 1–3 short-circuited fast.
- No new disk I/O (`go.sum` is already read for `ctx.go_sum_modules`).
- No new network I/O (offline path, by definition).

## Backward-compatibility contract

- Operators of mikebom-emitted Go SBOMs see ZERO behavior change in cache-populated configurations. Step 5 only fires for transitives that steps 1–3 didn't claim.
- Operators of mikebom-emitted Go SBOMs in offline-cache-empty configurations see strictly MORE components in their dep edges (root → ≥130 vs root → 31 today). Pre-091 SBOMs that passed downstream consumers continue to pass; the additions don't violate any existing PURL or relationship contract.
- The per-component discriminator (`mikebom:resolver-step = go-sum-fallback`) is additive — downstream consumers that don't read that property are unaffected. Consumers that DO read it now see a new value (`go-sum-fallback`) in addition to existing values.
