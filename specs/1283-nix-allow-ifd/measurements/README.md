# Measurements behind 1283

Collected 2026-11-11 on aarch64-darwin with Determinate Nix 3.20.0 (nix 2.34.6), waybill main@`1303ba82`, grype 0.120.0 (DB 2026-11-08), osv-scanner 2.7.0 and sbomnix 1.9.0.

| file | what it is |
|---|---|
| `stage2-analyze.json` | Recall-corpus stage 2: openai and slacklinker. Truth, W/WR recall, misses by cause. |
| `stage2-classify.tsv` | Every sbomnix grype finding on those targets, classified right-product / name-collision / questionable, with CVE text excerpts. |
| `stage2-target-shas.txt` | Pinned commits. |
| `slack-web-degraded.stderr` | waybill's WARN when slack-web's closure degrades on import-from-derivation. |
| `sbomnix-and-host.txt` | How sbomnix 1.9.0 evaluates and builds a flake reference. It contains no `allow-import-from-derivation` setting, so it inherits nix's default. Also the host's IFD default (`true`) and sandbox setting (`false`, nix's macOS default). |

## Results

| flake | packaging | IFD needed | waybill recall | note |
|---|---|---|---|---|
| MercuryTechnologies/moat@d0690555 | cabal2nix, pre-generated | no | passes | from the 1423 final check |
| MercuryTechnologies/slack-web@7b46273d | callCabal2nix | yes | 6/88 (7%) | 82 right-product sbomnix runtime findings missed; 22 sbomnix name collisions excluded |
| MercuryTechnologies/openai@8a357137 | override overlay + `packageSourceOverrides` | yes | 7/59 (12%) | all 52 misses "no closure"; they reach the output through GHC's toolchain (`nix why-depends`) |
| MercuryTechnologies/slacklinker@72af26a7 | cabal2nix via IFD | yes | not measurable here | the build fails on darwin (`disallowedReferences`); its CI builds x86_64-linux only |

The full stage 1 (nixpkgs packages) and stage 2 artifacts, including SBOMs, scanner outputs and scripts, are in the session's `recall-corpus/` work directory; its `README.md` lists every command. The probe will be committed with this feature's recall measurement.
