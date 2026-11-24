# Wire contract: closure-resolved tiers

## Documents

- **Per component:** `waybill:sbom-tier` changes from `design` to `build` on the two groups the data model defines. No other field, PURL, version, scope or edge changes.
- **Document level:** CycloneDX `metadata.lifecycles` and SPDX 3 `software_sbomType` are aggregated from component tiers, and change as a consequence.
- **No new catalog row.**
- **Unchanged:** every scan without `--nix-closure` (FR-007).

## Fixture cases (`waybill-cli/tests/closure_resolved_tier.rs`, stub `nix`)

| Case | Tree and closure | Expected |
|---|---|---|
| P1 | A Haskell project whose package is the evaluated attribute, with outputs examined (C214 `read`) | the project package is `build`; C203 present |
| P2 | As P1, outputs absent (C214 `absent`, moat's case) | `build` |
| P3 | A second repository package the closure does not evaluate (moat's `readme`) | stays `design` |
| B1 | The manifest declares `base`; the closure's GHC ships it | `base` is `build`, with C218 naming the closure's compiler |
| B2 | A boot library the manifest does not declare, synthesised by the closure | `build`, as today |
| B3 | The manifest declares a non-boot dependency the closure does not hold, with no version from any source | stays `design` |
| R1 | `--root-name` over P1 | the root's direct dependency is `build`; everything below it is reachable without passing through a `design` node |
| B4 | A boot library of the bootstrap `ghc-binary`, synthesised by the closure | `build`, untouched |
| B5 | The manifest pins a boot-library name to a version outside the boot table (no C218) | tier unchanged |
| T1 | P1 and B1's tree scanned with `--tier design-only` | the re-tiered project package and boot library are absent; P3's `readme` is present |
| N1 | The same project scanned without `--nix-closure` | byte-identical to `main` (control) |

Every case checks:
- `waybill sbom parity-check` reports no gap;
- the three formats agree on each component's tier;
- every `bom-ref` is unique.

## I11 cases (unit tests in `invariants/checks/closure_resolved_tier.rs`)

| Case | Document | Verdict |
|---|---|---|
| V1 | A closure document whose evaluated package is `design` with C203 | fail (a) |
| V2 | A closure document with a `design` component carrying C218 | fail (a) |
| V3 | A closure document whose root depends only on `design` components | fail (b) |
| V4 | A closure document with only `readme`-like `design` components (no C203, no C218) beside a `build` root dependency | pass |
| V5 | A manifest-only document full of `design` | not applicable |

**Positive control:** on `main`'s five closure goldens, I11 fails on moat, openai, slack-web and slacklinker, and passes on shellcheck.
