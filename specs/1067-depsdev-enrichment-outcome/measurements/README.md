# Measurements for #1058 (2026-10-04)

waybill `1bfbfd22` (debug), online, default enrichment, on cached public-corpus
checkouts. Probe: `probe_outcomes.sh`; raw output: `counts.txt`.

| repo | components | queried | record found | enriched | queried, nothing usable | never queried |
|---|---:|---:|---:|---:|---:|---:|
| express (npm) | 369 | 369 | 369 | 4 | 0 | 0 |
| flask (pypi) | 108 | 108 | 105 | 74 | 3 | 0 |
| cobra (go) | 7 | 8 | 7 | 7 | 1 | 0 |
| opentelemetry-go | 334 | 333 | 303 | 302 | 30 | 1 |
| ripgrep (cargo) | 68 | 61 | 61 | 61 | 0 | 7 |
| guice (maven) | 109 | 46 | 43 | 35 | 3 | 63 |

How the columns are derived:
- **queried** = `attempted`;
- **record found** = `matched`;
- **queried, nothing usable** = `attempted − matched − unqueried_offline`;
- **never queried** = components − `attempted`.

## Findings

- **opentelemetry-go's misses are placeholder versions.** 28 of its 31 unlicensed Go modules are the project's own workspace modules at `v0.0.0-unknown`. waybill queries deps.dev for them, which can never return a record, and the 404 is indistinguishable from "deps.dev has no data". This is the issue's silent-miss shape, found in a real repository.
- **guice's 63 never-queried components** include 48 maven components. All 94 maven components carry versions, and 66 already have licences from the POM. Plan research must establish which skip rule excludes them; the numbers suggest "already licensed" is one.
- **Offline is the norm in tests and the corpus.** Corpus scans run `--offline` (`corpus_harness_195/harness.rs:215`), as do 254 of 322 integration-test files. Any per-component signal emitted for offline scans would touch almost every component of almost every golden.
