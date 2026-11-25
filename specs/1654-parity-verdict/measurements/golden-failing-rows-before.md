# Failing rows per corpus golden at main 614e3c2 (exact, from the binary)

From a temporary, uncommitted instrument in `cli/parity_cmd.rs` that wrote each failing row's three value sets, run over the 28 goldens (`golden_parity.sh` layout).

| target | failing rows |
|---|---|
| go-cobra | G1 |
| go-opentelemetry | B1, B2, G1 |
| haskell-aeson | A7, B1, B2, G1 |
| haskell-language-server | A7, B1, B2, G1 |
| haskell-security-advisories | B1, B2, G1 |
| hpack-hs-opentelemetry | A7, B1, B2, G1 |
| hpack-hs-opentelemetry-no-generated-cabal | A7, B1, B2, G1 |
| hpack-hs-temporal-sdk | A7, G1 |
| hpack-lootbox | A7, G1 |
| hpack-lootbox-no-generated-cabal | A7, G1 |
| hpack-ogmios | A7, B1, B2, G1 |
| hpack-ogmios-no-generated-cabal | A7, B1, B2, G1 |
| image-postgres16 | A7, A8, G1 |
| maven-guice | A7, B1, B2, G1 |
| nix-closure-moat | A7, B1, B2, E1, G1 |
| nix-closure-openai | A7, B1, B2, E1, G1 |
| nix-closure-shellcheck | A7, B1, B2, E1, G1 |
| nix-closure-slack-web | A7, B1, B2, E1, G1 |
| nix-closure-slacklinker | A7, B1, B2, E1, G1 |
| npm-express | G1 |
| pants-clojure-polyglot | G1 |
| pants-example-django | G1 |
| pants-example-golang | G1 |
| pants-example-javascript | G1 |
| pants-example-jvm | G1 |
| pants-example-python | G1 |
| python-flask | B1, B2, G1 |
| rust-ripgrep | G1 |

Totals: {'A7': 16, 'A8': 1, 'B1': 15, 'B2': 15, 'E1': 5, 'G1': 28}; 80 in all.

The instrument was temporary and is not in the tree: `git diff main -- waybill-cli/src/cli/parity_cmd.rs` was empty when T001 ran.
