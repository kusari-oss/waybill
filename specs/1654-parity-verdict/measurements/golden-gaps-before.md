# Corpus golden parity gaps at main 614e3c2

`golden_parity.sh target/debug/waybill waybill-cli/tests/fixtures/public_corpus`: per target, `summary.parity_gaps`, then the universal rows whose presence or count differ, written as CycloneDX / SPDX 2.3 / SPDX 3 (`-` = absent). A differing row is not necessarily a failing one: see spec Background.

```
go-cobra | 1 | A12:8/8/15 G1:1/-/1
go-opentelemetry | 3 | A12:333/333/664 B1:258/271/271 B2:23/10/10 G1:1/-/1
haskell-aeson | 4 | B1:95/88/88 B2:15/22/22 G1:1/-/1
haskell-language-server | 4 | A7:12/12/13 B1:2141/2331/2335 B2:267/73/73 G1:1/-/1
haskell-security-advisories | 3 | B1:1413/1533/1533 B2:159/39/39 G1:1/-/1
hpack-hs-opentelemetry-no-generated-cabal | 4 | A7:2/2/- B1:428/420/421 B2:47/54/54 G1:1/-/1
hpack-hs-opentelemetry | 4 | A7:2/2/- B1:428/420/421 B2:47/54/54 G1:1/-/1
hpack-hs-temporal-sdk | 2 | A7:1/1/- G1:1/-/1
hpack-lootbox-no-generated-cabal | 2 | A7:1/1/- G1:1/-/1
hpack-lootbox | 2 | A7:1/1/- G1:1/-/1
hpack-ogmios-no-generated-cabal | 4 | A7:1/1/- B1:142/135/135 B2:19/26/26 G1:1/-/1
hpack-ogmios | 4 | A7:1/1/- B1:142/135/135 B2:19/26/26 G1:1/-/1
image-postgres16 | 3 | A7:258/80/80 A8:258/80/- A12:145/145/289 G1:1/-/1
maven-guice | 4 | A12:46/46/130 B1:27/27/33 B2:35/29/29 G1:1/-/1
nix-closure-moat | 5 | A7:26/31/31 B1:1301/8000/8000 B2:6808/109/109 D1:70/2/2 E1:-/1/1 G1:1/-/1
nix-closure-openai | 5 | A7:29/35/35 B1:2038/10417/10417 B2:8454/75/75 D1:56/2/2 E1:-/1/1 G1:1/-/1
nix-closure-shellcheck | 5 | A7:27/33/33 B1:596/13182/13182 B2:12587/1/1 D1:71/2/2 E1:-/1/1 G1:1/-/1
nix-closure-slack-web | 5 | A7:27/31/32 B1:1791/9966/9980 B2:8332/143/143 D1:57/3/3 E1:-/1/1 G1:1/-/1
nix-closure-slacklinker | 5 | A7:31/38/39 B1:2977/15394/15394 B2:12597/180/180 D1:78/3/3 E1:-/1/1 G1:1/-/1
npm-express | 1 | G1:1/-/1
pants-clojure-polyglot | 1 | A12:51/51/115 G1:1/-/1
pants-example-django | 1 | A12:33/33/66 G1:1/-/1
pants-example-golang | 1 | A12:4/4/7 G1:1/-/1
pants-example-javascript | 1 | A12:303/303/400 G1:1/-/1
pants-example-jvm | 1 | A12:27/27/80 G1:1/-/1
pants-example-python | 1 | A12:10/10/20 G1:1/-/1
python-flask | 3 | A12:108/108/216 B1:53/66/222 B2:169/-/- G1:1/-/1
rust-ripgrep | 1 | G1:1/-/1
```

Total: 80 parity gaps across 28 goldens.
