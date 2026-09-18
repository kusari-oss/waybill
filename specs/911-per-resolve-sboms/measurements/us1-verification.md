# US1 verification (T021, T022)

## After

```
pkg:generic/app                            ["app"]
pkg:generic/tools                          ["tools"]
pkg:pypi/waybill-fixture-common@1.0.0      ["app","tools"]   <- was ["app"]
pkg:pypi/waybill-fixture-consumer-a@1.0.0  ["app"]
pkg:pypi/waybill-fixture-consumer-b@1.0.0  ["tools"]
pkg:pypi/waybill-fixture-shared@1.0.0      ["app"]
pkg:pypi/waybill-fixture-shared@2.0.0      ["tools"]
```

## T021 — which tests have teeth, and which are guards

Labelled rather than counted, because a test that passes on both sides of the
change proves nothing about the change.

| Test | Pre-change | Kind |
|---|---|---|
| `membership_names_every_resolve_that_pins_the_package` | **FAILED** | defect test (SC-001) |
| `a_single_resolve_component_uses_the_array_form_too` | **FAILED** | defect test (SC-004) |
| `no_component_emits_the_pre_911_bare_string_form` | **FAILED** | defect test (C-1a) |
| `a_component_in_two_resolves_reaches_both_pinnings_of_its_dependency` | passed | **guard** — emergent behaviour, see below |
| `membership_is_identical_across_repeated_scans` | passed | **guard** — singular was already deterministic |

Three of five have teeth. The two guards are worth keeping for opposite
reasons: one protects behaviour nothing had stated, the other protects a
property the change could plausibly have broken.

## Cross-format carriage — and a spec correction

| format | value |
|---|---|
| CycloneDX | `"[\"app\",\"tools\"]"` — JSON-in-string |
| SPDX 2.3 | `{"field":"waybill:pants-resolve","value":["app","tools"]}` |
| SPDX 3 | same as SPDX 2.3 |

FR-006 originally said membership must be expressed *identically* across
formats. That is not achievable: CycloneDX spec'es `properties[].value` as a
string, so an array can only be carried encoded there. Reworded to fix the
**decoded** value, which is what a consumer acts on and what the parity
extractors already compare. My first version of the single-resolve test
asserted a native JSON array and failed against correct output.

## Test churn — re-authored, not loosened

12 assertions across 4 targets broke. Rather than rewrite each to expect a
JSON literal, each file's **extractor** now decodes, so the assertions keep
naming a resolve directly (`Some("default")`, not `Some("[\"default\"]")`).
Two more surfaced in the full suite that per-target runs had missed, because
those runs happened before the writers changed.

## T022 — the monorepo re-measurement

Still the issue author's to run; see `baseline.md`. Recorded, not faked.

## Gate

`./scripts/pre-pr.sh` — **304 binaries, 5883 passed, 0 failed**, clippy clean.
