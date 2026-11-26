# Spec 1662 measurements

Re-run with `measurements/measure.sh <binary> <label> <work-dir>`. The flags are the corpus harness's (`--offline`, `--root-name`, `--root-version`). A `-no-generated-cabal` variant deletes every `*.cabal` with `hpack` in its first four lines, as `remove_hpack_generated_cabal` does. Each target was fetched at its corpus pin (`manifest.rs`) and checked with `git rev-parse HEAD`.

## Baseline: `main` at `6ea77ef5` (waybill 0.18.0)

| target | `pkg:hackage` | `json-via-show` | `package.yaml` in C200 `manifests-seen` |
|---|---|---|---|
| haskell-language-server | 360 | 0 | 1 (a test fixture with no `name:`, `not-a-manifest`) |
| hpack-hs-opentelemetry | 359 | 0 | |
| hpack-hs-opentelemetry-no-generated-cabal | 359 | 0 | |
| hpack-lootbox | 62 | 0 | |
| hpack-lootbox-no-generated-cabal | 62 | 0 | |
| hpack-hs-temporal-sdk | 215 | 0 | |
| hpack-ogmios | 103 | 1 | 8, including `modules/json-via-show/package.yaml` |
| hpack-ogmios-no-generated-cabal | 103 | 1 | 8 |

The `.cabal` files removed for the variants were 24 of 37 in hs-opentelemetry (13 hand-written ones stay), 7 of 7 in lootbox and 8 of 8 in ogmios. Per-target sets are in `main/`.

**Expected after the change** (a prediction, checked in T011):
- ogmios and its variant lose `json-via-show`, both its component and its C200 entries.
- haskell-language-server's C200 loses its one fixture `package.yaml`: `plugins/hls-cabal-plugin/test/testdata/…` is not in `cabal.project`'s `packages:`. Its component set doesn't change, because that manifest was never read.
- Nothing else changes.

## After: branch, release build

The first cut used `cabal.project` alone, as #1032 does for `.cabal`. It also dropped two packages that another build definition does build:
- hs-opentelemetry's `examples/otlp-demo`: commented out of `cabal.project`, but listed in `stack.yaml`'s `packages:`;
- hs-temporal-sdk's `tools/tix-to-markdown`: outside `packages: */*.cabal`, which matches one directory level, and built by `nix/packages/tix-to-markdown.nix`.

Decision (maintainer): membership is the union of `cabal.project` and a `stack.yaml` in the same directory, for both readers. The table below is from that build.

| target | `pkg:hackage` main → branch | difference | reason |
|---|---|---|---|
| haskell-language-server | 360 → 360 | C200 absent (was: 1 seen, 1 unread) | its only `package.yaml` is a test fixture under `plugins/hls-cabal-plugin/test/testdata/`, not declared; C200 is written only when a manifest is seen |
| hpack-hs-opentelemetry | 359 → 359 | C200 `generated-cabal-suppressed` 23 → 24 | `stack.yaml` now declares `examples/otlp-demo`, so #1032 keeps `otlp-demo.cabal`, and its `package.yaml` suppresses it as before: still one component |
| hpack-hs-opentelemetry-no-generated-cabal | 359 → 359 | none | |
| hpack-lootbox, and its variant | 62 → 62 | none | |
| hpack-hs-temporal-sdk | 215 → 213 | `tix-to-markdown@2025.10.1.0` and `hpc` removed; C200 seen/read 7 → 6, includes-resolved 6 → 5 | `tools/tix-to-markdown` is in neither `cabal.project` (`*/*.cabal` is one level) nor a `stack.yaml`; only a Nix file builds it. `hpc` is its dependency alone. Accepted; a Nix-built package isn't recognized as a member (#1731) |
| hpack-ogmios, and its variant | 103 → 102 | `json-via-show@2.0.0` removed; C200 seen/read 8 → 7, includes-resolved 16 → 14 | the fix: `server/cabal.project` doesn't list `modules/json-via-show`, and there's no stack file |

Unexplained differences: 0.

## Corpus goldens (T014)

Run 45848548209 regenerated the goldens on this branch (`public-corpus.yml`, `regen_goldens=true`, scanned commit `4fe0d064`). It reports 15 golden files changed. They are the five targets in the table above, each in CycloneDX, SPDX 2.3 and SPDX 3. No other target changed, and no golden file was added or removed.

The sorted `purl` difference and the C200 counts in each changed CycloneDX golden match the table, with one more line:
- **hs-temporal-sdk also loses the versionless `pkg:hackage/filepath`.** In the committed golden its only dependent is `tix-to-markdown@2025.10.1.0`.
- **Locally it stayed.** `measure.sh` doesn't set `WAYBILL_NIXPKGS_CACHE` the way the harness does, so the local scans read a local nixpkgs cache. That cache resolved a transitive set (`conduit@1.3.6` and 12 others) that also depends on `filepath`.
- **The comparison still holds.** `main` and the branch were measured in the same environment, so the comparison between them is valid, but the absolute counts above aren't the goldens' counts.
