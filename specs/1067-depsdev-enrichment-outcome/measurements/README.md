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

## After implementation (2026-10-04)

waybill at the m1067 branch head (debug), online, fresh deps.dev disk cache
per scan. Probe: `probe_outcomes_emitted.sh`; raw output: `outcomes_emitted.txt`.

| repo | attempted (before → after) | matched | C192 | C191 tallies |
|---|---|---:|---|---|
| opentelemetry-go | 333 → 305 | 303 → 303 | `{"absent":2,"not-queried:incomplete-coordinate":28,"not-queried:unsupported-ecosystem":1}` | `absent` 2, `incomplete-coordinate` 28 |
| express | 369 → 369 | 369 → 369 | `{"not-queried:unsupported-ecosystem":1}` | none |

- **SC-002:** the 28 placeholder-version requests on opentelemetry-go are gone
  (333 → 305 attempted). The matched count is unchanged, so no real lookup
  was lost.
- **SC-005:** for every C191 value, the C192 count equals the number of
  components carrying it, on both repositories.
- **express:** no component carries C191. The one `unsupported-ecosystem`
  count is a file-tier component (`run`, no PURL in CycloneDX), which deps.dev
  cannot index. Counting it is the specified behaviour (clarification Q4,
  FR-009: that component is un-enriched).
