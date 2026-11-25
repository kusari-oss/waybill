# Failing rows per corpus golden, read from the verdict field (T004)

Produced by `golden_parity.sh` with the spec 1654 binary, before any US3 fix. It matches `golden-failing-rows-before.md` (from the temporary instrument) exactly: 28 targets, 80 failing rows, no target differing. The script also asserts, per target, that the number of `fail` rows equals `summary.parity_gaps`.

| target | parity_gaps | failing rows | G1 (masking, R3) |
|---|---|---|---|
| go-cobra | 1 | — | fail |
| go-opentelemetry | 3 | B1, B2 | fail |
| haskell-aeson | 4 | A7, B1, B2 | fail |
| haskell-language-server | 4 | A7, B1, B2 | fail |
| haskell-security-advisories | 3 | B1, B2 | fail |
| hpack-hs-opentelemetry-no-generated-cabal | 4 | A7, B1, B2 | fail |
| hpack-hs-opentelemetry | 4 | A7, B1, B2 | fail |
| hpack-hs-temporal-sdk | 2 | A7 | fail |
| hpack-lootbox-no-generated-cabal | 2 | A7 | fail |
| hpack-lootbox | 2 | A7 | fail |
| hpack-ogmios-no-generated-cabal | 4 | A7, B1, B2 | fail |
| hpack-ogmios | 4 | A7, B1, B2 | fail |
| image-postgres16 | 3 | A7, A8 | fail |
| maven-guice | 4 | A7, B1, B2 | fail |
| nix-closure-moat | 5 | A7, B1, B2, E1 | fail |
| nix-closure-openai | 5 | A7, B1, B2, E1 | fail |
| nix-closure-shellcheck | 5 | A7, B1, B2, E1 | fail |
| nix-closure-slack-web | 5 | A7, B1, B2, E1 | fail |
| nix-closure-slacklinker | 5 | A7, B1, B2, E1 | fail |
| npm-express | 1 | — | fail |
| pants-clojure-polyglot | 1 | — | fail |
| pants-example-django | 1 | — | fail |
| pants-example-golang | 1 | — | fail |
| pants-example-javascript | 1 | — | fail |
| pants-example-jvm | 1 | — | fail |
| pants-example-python | 1 | — | fail |
| python-flask | 3 | B1, B2 | fail |
| rust-ripgrep | 1 | — | fail |
