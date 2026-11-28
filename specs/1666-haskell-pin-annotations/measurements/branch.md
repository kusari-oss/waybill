# Spec 1666 corpus measurement (T014)

**Main:** the committed goldens at 46506106, measured with `measure.sh` and written to `main/`.

**Branch:** the goldens from `public-corpus.yml` run 46092931861 (`regen_goldens=true`), written to `branch/`. Run 46091053495, the first dispatch, produced the same measurement. It failed on `xtask nix-oracle`, which used C172 as the disclosure for main modules. The oracle now classifies main modules by source type (commit "nix-oracle recognises the project's own packages").

The diff is normalized and sorted (`LC_ALL=C sort`). Every line is explained:

| Target | Change | Why |
|---|---|---|
| haskell-language-server | C172 removed on ghcide, haskell-language-server, hls-graph, hls-plugin-api and hls-test-utils (all `hackage-main-module`, 2.15.0.0 against 2.13.0.0); C174 11 → 6 | FR-003 |
| haskell-language-server | the 6 `"emitted": null` entries are unchanged | US3 scenario 3: the closure did not supply them |
| haskell-security-advisories | C172 removed on cvss, hsec-core, hsec-tools and osv (main modules); C174 4 → 0 | FR-003 |
| nix-closure-slack-web | C172 removed on slack-web@2.4.0.0 (main module) | FR-003 |
| nix-closure-slack-web | C172 removed on hspec-discover@2.11.14, built at nixpkgs' version; C221 still present; C174 2 → 0 | FR-005 |
| nix-closure-slacklinker | slack-web@2.3.0.0 C172 rewritten: `emitted` 2.3.0.0, closure note, no `requirement-ranges`; C174 stays 1 | FR-005 |

- **Unreachable components:** the orphan-reason sections are identical in every target. Neither side has an unreachable PURL'd component without a reason.
- **Goldens committed:** 12 files across the 4 targets above. When every C172 and C174 entry is removed from both sides, each of the 12 is byte-identical to its committed golden.
- **Golden not committed:** `hpack-hs-temporal-sdk/spdx-3.json`. Its only difference is the #1738 stray `Organization "Microsoft"` element.
