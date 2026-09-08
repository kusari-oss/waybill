# Contract: `xtask quality` CLI

**Invocation**: `cargo run -p xtask --release -- quality [FLAGS]`
**Producer**: `xtask::quality::mod`

## C-1 — Flags

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--filter <GLOB>` | repeatable | none | Restrict to matching target names; multiple flags union (FR-027). Mirrors `xtask bench --filter`, `*` the only metaclass. An empty match set is **not** an error — it reports "nothing selected" and exits 0. |
| `--corpus <PATH>` | path | `xtask/corpus/quality-corpus.toml` | Override the corpus file. |
| `--output <PATH>` | path | `target/quality/run-<sha12>.json` | Override the report path. |
| `--cache-dir <PATH>` | path | `~/.cache/waybill/quality-corpus` | Override the repository cache root. |
| `--waybill-bin <PATH>` | path | `target/release/waybill` | Override the binary under measurement. |
| `--timeout-secs <N>` | u64 | corpus `default_timeout_secs` | Override every target's scan budget. |
| `--no-gate` | bool | false | Measure and report, but always exit 0. For range-authoring runs (US1 without US2). |
| `--refresh` | bool | false | Ignore cached checkouts and re-fetch. |

**C-1.1** — `--no-gate` suppresses only the *exit code*. Violations are still computed and still
printed, so an author can see what would have failed while authoring bounds.
**C-1.2** — No flag can weaken FR-016: a missing `sbomqs` fails the run even under `--no-gate`,
because the run produced no quality data at all.

## C-2 — Order of operations

1. Parse and validate the corpus. Any configuration error ⟹ report all, **exit 2**, fetch nothing.
2. Verify `sbomqs` is present and matches `sbomqs_version`. Absent ⟹ exit 1 with an explicit
   message. Version mismatch ⟹ record it and continue (a warning, not a failure — the score is
   still comparable, just noted).
3. For each selected target: fetch (cache-hit skips), scan with a timeout, analyse, score.
4. Evaluate every measurement against its expectation — all of them, no short-circuit (FR-018).
5. Write the JSON report atomically (temp file + rename), then print the human summary.
6. Exit per [quality-report.md § C-5](./quality-report.md).

**C-2.1** — The report is written **before** the exit-code decision, so a failing run still
leaves a report behind (FR-029).

## C-3 — Fetch behaviour

```
git init <cache>/<name>/<sha>
git -C <dir> remote add origin <url>
git -C <dir> fetch --depth 1 origin <sha>
git -C <dir> checkout FETCH_HEAD
```

**C-3.1** — No `--recurse-submodules`. Nested sub-repositories stay empty by design (research R6).
**C-3.2** — A successful checkout drops a marker file; its presence is the cache-hit test.
**C-3.3** — A fetch failure marks that target `unmeasurable`, continues with the rest, and fails
the run (FR-007).

## C-4 — Scan invocation

```
<waybill-bin> --offline sbom scan \
  --path <checkout> \
  --format cyclonedx-json \
  --output cyclonedx-json=<tmp>/<name>.cdx.json \
  --root-name <name> --root-version <sha12>
```

**C-4.1** — `--offline` is a **global** flag and precedes `sbom scan`. This matches
`waybill-cli/tests/corpus_harness_195/harness.rs:184`.
**C-4.2** — Only this subprocess is timed (FR-009).
**C-4.3** — No tier filter and no `--file-inventory` override is passed. The corpus measures
waybill as an ordinary user invokes it (research R5).
**C-4.4** — `$GOMODCACHE` is pinned to an empty per-run directory so Go edge counts do not drift
with whatever the host happens to have cached (research R2).

## C-5 — Scoring invocation

```
sbomqs score --json <cdx>   →  files[0].sbom_quality_score
```

**C-5.1** — The binary is located by `WAYBILL_SBOMQS_BIN` then `$PATH`, matching
`waybill-cli/tests/sbomqs_parity.rs:33`.
**C-5.2** — Unlike `sbomqs_parity.rs`, absence is **not** a silent skip here. That test may skip
because it is one signal among many; this command's entire purpose is the score.
