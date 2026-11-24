# T014, T015: corpus goldens, regenerated in CI

- **Regeneration dispatch:** `public-corpus.yml -f branch=1650-closure-resolved-tier -f regen_goldens=true`, run 45315425551.
- **Scanned commit:** the log names `896de59337fe2ccba5b78bbefd85a68e0ff34cbd`.
- **Result:** success. Corpus suite `ok. 179 passed; 0 failed` (174 before, plus I11's 5 unit tests), plus a 3-test suite.
- **I11 on fresh output:** the live invariant matrix shows `.` (pass) for all five closure targets.

## What changed (SC-004)

Of 87 regenerated goldens, **75 are byte-identical**, including nix-closure-shellcheck and every manifest-only and non-Haskell target. **12 differ**: moat, openai, slack-web and slacklinker, in all three formats. All 12 were installed.

### CycloneDX, traced component by component

```
nix-closure-moat: retiered=20 with-evidence=20 only-tier-changed=20 other-component-diffs=[] edges-identical=True metadata-keys-differing=[] doc-properties-differing=[] lifecycles [{'phase': 'build'}, {'phase': 'design'}, {'phase': 'pre-build'}] -> [{'phase': 'build'}, {'phase': 'design'}, {'phase': 'pre-build'}]
nix-closure-openai: retiered=22 with-evidence=22 only-tier-changed=22 other-component-diffs=[] edges-identical=True metadata-keys-differing=['lifecycles', 'properties'] doc-properties-differing=['waybill:graph-completeness-reason'] lifecycles [{'phase': 'build'}, {'phase': 'design'}, {'phase': 'pre-build'}] -> [{'phase': 'build'}, {'phase': 'pre-build'}]
nix-closure-slack-web: retiered=24 with-evidence=24 only-tier-changed=24 other-component-diffs=[] edges-identical=True metadata-keys-differing=['lifecycles', 'properties'] doc-properties-differing=['waybill:graph-completeness-reason'] lifecycles [{'phase': 'build'}, {'phase': 'design'}, {'phase': 'pre-build'}] -> [{'phase': 'build'}, {'phase': 'pre-build'}]
nix-closure-slacklinker: retiered=24 with-evidence=24 only-tier-changed=24 other-component-diffs=[] edges-identical=True metadata-keys-differing=['lifecycles', 'properties'] doc-properties-differing=['waybill:graph-completeness-reason'] lifecycles [{'phase': 'build'}, {'phase': 'design'}, {'phase': 'post-build'}, {'phase': 'pre-build'}] -> [{'phase': 'build'}, {'phase': 'post-build'}, {'phase': 'pre-build'}]
```

**Every changed component is a re-tier, and nothing else changed:**
- each moves from `design` to `build`;
- each carries C203 `exempt-evaluated-package` or C218;
- with the tier removed, each is identical before and after;
- the components and edges are identical.

The counts (20, 22, 24, 24) equal the census (`census.md`).

### Document-level differences (consequences of the tiers)

- **`metadata.lifecycles`**, and SPDX 3's `software_sbomType` with its "Observed lifecycle phases" comment, lose `design` on openai, slack-web and slacklinker. moat keeps it: `readme` is still `design`. Predicted in research R7.
- **`waybill:graph-completeness-reason`** loses `transitive-edges-unresolvable: hackage` on the same three targets; moat keeps it, for the same reason. Not predicted in R7. Milestone 177 raises the clause for a `design` or `analyzed` component with no `source`/`build`/`deployed` same-package counterpart. The re-tiered components are now `build`, as their synthesised siblings already were, and those never raised it.
  - **The clause was true for another reason.** Boot libraries are emitted as leaves: 1 of 40 has an outgoing edge in openai, and `base` has none although it depends on `ghc-prim`.
  - **shellcheck, at `main`, already reports `complete`** over 39 leaf boot libraries, so the gap predates this spec. Filed as #1688.

### SPDX

- **SPDX 2.3:** every changed string is a `waybill:sbom-tier` annotation (2 per re-tiered component: 40, 44, 48, 48) or `waybill:graph-completeness-reason`.
- **SPDX 3:** the same, plus `software_sbomType` and the lifecycle comment above.

## SC-002 on the installed goldens

| Target | `design` | Reachable from root | Reachable without `design` nodes |
|---|---|---|---|
| nix-closure-moat | 1 ['readme'] | 1,135 | 1,134 |
| nix-closure-openai | 0 [] | 1,362 | 1,362 |
| nix-closure-shellcheck | 0 [] | 2,085 | 2,085 |
| nix-closure-slack-web | 0 [] | 1,235 | 1,235 |
| nix-closure-slacklinker | 0 [] | 2,441 | 2,441 |

Exactly research R7's prediction. On `main` the last column read 0 in the four project targets.

`golden_matrix_is_evaluable_and_gated` passes locally on the installed goldens, with I11 applicable on 5 of 20 targets and failing on none.
