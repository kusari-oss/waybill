# Quickstart — Pants JavaScript/npm corpus regression gate

## Prerequisites

- Workspace at `675-pants-js-corpus` branch
- `git` on PATH
- `WAYBILL_RUN_PUBLIC_CORPUS=1` env var (default `cargo test` does NOT run this target)
- Network access (target clones `github.com/kusari-sandbox/example-javascript`)

## Files that will be created / modified

| File | Change |
|---|---|
| `waybill-cli/tests/corpus_harness_195/manifest.rs` | +1 `CorpusTarget` entry |
| `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` | +1 assertion function |
| `waybill-cli/tests/corpus_harness_195/layer2_golden.rs` | +1 dispatch to `filter_*_to_js` |
| `waybill-cli/tests/corpus_harness_195/js_filter.rs` | NEW file — 3 filter functions + unit tests |
| `waybill-cli/tests/corpus_harness_195/mod.rs` | +1 `pub mod js_filter;` |
| `waybill-cli/tests/public_corpus.rs` | +1 `#[test]` entry |
| `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/cdx.json` | NEW golden |
| `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/spdx-2.3.json` | NEW golden |
| `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/spdx-3.json` | NEW golden |

Plus one external artifact:

- Fork of `pantsbuild/example-javascript` at SHA `da76d5dbb407d82c136cfe8f18dc06f3c8a440e5` into `kusari-sandbox/example-javascript`.

## End-to-end local verification

### Step 1 — Fork the upstream repo

```bash
gh repo fork pantsbuild/example-javascript --org kusari-sandbox --clone=false
```

Verify the pinned SHA is present in the fork:

```bash
git ls-remote --heads https://github.com/kusari-sandbox/example-javascript main
# Expected: da76d5dbb407d82c136cfe8f18dc06f3c8a440e5  refs/heads/main
```

### Step 2 — Compile the test-infra changes

```bash
cargo test --test public_corpus --no-run
```

Confirms the new manifest entry, layer 1 function, and `js_filter` module compile cleanly.

### Step 3 — Verify audits still pass (no network)

```bash
cargo test --test public_corpus
```

The four manifest audits (`public_only_audit`, `public_hostname_allowlist`, `no_credentials_required`, `cross_ecosystem_coverage_check`) plus the seven per-target skip-tests should all pass. The new `corpus_pants_example_javascript` returns immediately because `WAYBILL_RUN_PUBLIC_CORPUS` is unset.

### Step 4 — Generate goldens

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus corpus_pants_example_javascript
```

This will:
1. Clone the fork into `~/.cache/waybill/corpus/<source-id>/<pinned-sha>/`
2. Scan it with the released `waybill` binary (via `env!("CARGO_BIN_EXE_waybill")`)
3. Apply the existing masking pass
4. Apply the new `filter_*_to_js` pass
5. Write the 3 goldens to `waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/`

Expected: test passes with `1 passed`.

### Step 5 — Verify byte-identity across a second run

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus corpus_pants_example_javascript
```

(No `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS` this time.) The test compares emitted output against the freshly-written goldens. Expected: `1 passed`.

### Step 6 — Verify golden size fits SC-004

```bash
du -sh waybill-cli/tests/fixtures/public_corpus/pants-example-javascript/
# Expected: ≤ 500 KB
```

### Step 7 — Confirm layer 1 catches a regression

On a scratch branch, introduce a synthetic bug into the npm reader (e.g., early-return in `waybill-cli/src/scan_fs/package_db/npm/package_lock.rs` before packages are enumerated), rebuild, and re-run step 5. Expected: layer 1 assertion 1 (`npm-transitives-present-at-scale`) fails with the diagnostic naming the npm reader. Discard the scratch branch after verification.

### Step 8 — Confirm existing corpus targets stay byte-identical

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus
```

All 7 corpus targets (6 existing + 1 new) should pass. The 6 pre-existing targets MUST remain byte-identical to their pre-feature output (SC-002 zero-production-change guarantee).

### Step 9 — Full pre-PR gate

```bash
./scripts/pre-pr.sh
```

Expected: clippy + workspace tests both green. This is CI's exact command per `feedback_prepr_gate_full_output`.

## Common pitfalls

- **Fork must exist before nightly CI runs** — creating the corpus target entry that points at `github.com/kusari-sandbox/example-javascript` before the fork is created will cause nightly CI to fail with a `GitClone` `CorpusInfraError`. Order: fork first, PR second.
- **Golden regen without the JS filter would balloon size** — the raw emitted CDX is ~570 KB; without the filter the goldens would violate SC-004. Do NOT manually edit `layer2_golden.rs::compare_golden` to skip the filter dispatch during regen.
- **Layer 1 threshold tuning** — if a legitimate lockfile refresh drops the count below 250 (very unlikely for this fixture but possible), bump the threshold in `layer1_assertions.rs` — don't disable the assertion. Document the bump in the PR.
- **Do not remove Pants annotations from goldens manually** — Invariant 4 (`no-accidental-pants-annotations-on-npm`) enforces their absence. If option A (from issue #760) lands and adds them intentionally, remove Invariant 4 as part of that milestone AND regenerate goldens; do not "clean up" the goldens by hand.
