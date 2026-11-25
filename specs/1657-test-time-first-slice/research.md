# Research: cut serial test time, first slice

Read against `main` 60102ff5.

## R1: The SPDX 3 index (#1711)

**What the code does now.** `annotations_about(subject, field)` (`edit/spdx3.rs:81`) scans all of `@graph` and parses each Annotation's statement envelope. It has 8 call sites. Seven are in `components()` (`:144`–`:174`), up to 6 per component. The eighth is `doc_annotation` (`:476`).

**Decision.** Build one index, a `BTreeMap<(subject, field), Vec<String>>`, in a single pass over `@graph`. It is computed once per `components()` / `doc_annotation` call, or cached on the adapter. Either is fine as long as an edit that mutates `@graph` never reads a stale index: build it at the start of each read, and never across a mutation.

**Ordering must be preserved.** `annotations_about` returns values in graph order. The index pushes in the same order, so the output is byte-identical (FR-002).

**Verification.**
- `sbom_edit_*` tests stay green.
- A byte-identity check edits every input the suite edits, plus the five closure goldens under the i1454 recipe, with the before and after binaries, and compares the outputs (`measurements/edit_identity.sh`).
- Timing is taken 3 runs each, debug and release (`measurements/edit_timing.sh`).

## R2: The harness self-tests (#1712)

**Where the tests live.** `#[test]` functions sit in six harness files (grep count of `#[test]`): `cache.rs` 7, `expectations.rs` 13, `harness.rs` 1, `js_filter.rs` 12, `layer2_golden.rs` 17, `manifest.rs` 6, so 56 attributes. The `--list` count of 145 per consumer binary is the authority; the gap to 56 is explained in T001, for example by nested or generated tests. (Corrected at analyze: R2 first named four files.) The 17 consumers include the whole module through `#[path]`.

**Decision.** Move those test modules into one new integration test file, `tests/corpus_harness_self_tests.rs`. It includes the harness through the same `#[path]` and holds the tests. Any private item the tests reach is made `pub(crate)` within the harness. The consumers are unchanged except that they no longer compile the tests.

**Rejected:** a cfg flag set only for one binary. Cargo has no per-test-binary cfg without features, and a feature would touch the workspace manifest.

**Verification.** `--list` on all 18 binaries. Exactly one has `corpus_harness_195::` tests, 145 of them (`measurements/list_counts.sh`).

`dual_format_perf`: move `build_benchmark_fixture` and `ImageFile` into `tests/common/`, and drop `mod dual_format_perf;` from `holistic_parity.rs`.

## R3: Offline scans and the enforcement point (#1715)

**Facts.**
- No deps.dev base-URL override exists in `src` (grepped for `DEPS_DEV`; only the cache variables exist).
- The Go proxy client sets `.no_proxy()`. The deps.dev client does not, so reqwest honours `HTTPS_PROXY`.

**Decision.**
- **Helpers:** the shared helpers (`run_split_scan`, `common/nix_stub.rs`'s `scan`/`scan_to`, `common/installed_lib.rs`, `common/cmake_fetch.rs`) pass `--offline` where the test is not about enrichment.
  - Many already pass `--no-deps-dev`, which avoids deps.dev without implying offline. The review records which each file uses.
  - **Caution:** `--offline` also refuses the closure tier (memory: stub-nix repro). Closure helpers therefore keep `--no-deps-dev` and must not gain `--offline`. This is checked per file.
- **Enforcement:** the shared helpers set `HTTPS_PROXY=http://127.0.0.1:9` (and `HTTP_PROXY`). Any live deps.dev call then fails fast instead of passing slowly.
  - This needs no production change.
  - It is adopted only if T001 measures that a scan with this proxy and without `--offline` (a) does not hang, and (b) records the failure visibly (C191 / C192 enrichment outcome). That is the positive control.
  - If either fails, enforcement is dropped and the review list is the guard.

**Verification.** `measurements/offline-review.md` holds one row per candidate file: offline, `--no-deps-dev`, mocked, deliberately online with the reason, or not a scan. Before and after times are recorded per affected binary.

## R4: Measurement method (FR-010)

- **Per binary:** each affected binary runs alone 3 times on an idle machine, and min, median and max are recorded.
- **Whole suite:** one `cargo test --workspace --no-fail-fast` log each side. Per-binary times are taken from the log timestamps, the way #1718 measured them.
- **Labelling:** the sum is reported, not targeted. Figures from different machine states are never mixed in one table.
