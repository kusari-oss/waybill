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

**C-2.2** — Before measuring, the harness runs
`cargo build --release -p waybill --bin waybill`, so the corpus can never measure a stale
binary. The report stamps provenance from `git rev-parse HEAD` of the *source tree*, so a stale
binary would silently attribute one build's numbers to a different commit — and those numbers
can end up committed as authored ranges. Cargo also keys off mtime for path dependencies, so a
`git checkout` can still trigger a redundant rebuild — but cargo *rebuilds* where a hand-rolled
staleness check could only *refuse*, so the operator is never blocked. A genuine no-op costs
under a second. An explicit `--waybill-bin <path>` skips the
rebuild, since passing it is a statement of intent to measure a specific binary built elsewhere.
A failed build aborts the run rather than falling back to whatever artifact is on disk.

## C-3 — Fetch behaviour

```
git init <cache>/<name>/<sha>
git -C <dir> remote add origin <url>
git -C <dir> fetch --depth 1 origin <sha>
git -C <dir> checkout FETCH_HEAD
```

**C-3.1** — No `--recurse-submodules`. Nested sub-repositories stay empty by design (research R6).

**C-3.3** — Git LFS smudging is disabled (`GIT_LFS_SKIP_SMUDGE=1`) on every git invocation the
fetcher makes. Same rationale as C-3.1: without it, a checkout contains real content on a host
with `git-lfs` installed and 131-byte pointer files on a host without it, so the fixture is not
reproducible and no bound authored against it is meaningful. Observed on `pants-backend-ai`,
which stores `*.bin` / `*.so` under LFS — GitHub runners ship git-lfs, so CI measured 317 pkgs /
45 files against an author's 271 / 59 and the lane failed nightly from 2026-09-09 (issue #832).
Pinning smudge off matches the authored bounds; verified on `ubuntu-latest` that
`GIT_LFS_SKIP_SMUDGE=1` reproduces 271 / 59 exactly, so no re-baseline is required.

Consequence worth stating: corpus measurements describe the repository's **tracked source**, not
its LFS payload. A target whose interesting content lives in LFS will under-report, and that is
the deliberate trade for reproducibility.
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
**C-4.4** — `$GOMODCACHE`, `$GOPATH` **and** `$HOME` are all pinned to the same empty per-run
directory so Go edge counts do not drift with whatever the host happens to have cached
(research R2). All three are required: waybill's module-cache discovery falls back
`$GOMODCACHE` → `$GOPATH` → `$HOME/go/pkg/mod`, so pinning only the first lets a warm host
cache leak in through the others — the defect that made the first real CI run disagree with
local measurements on both Go targets.

## C-5 — Scoring invocation

```
sbomqs score --json <cdx>   →  files[0].sbom_quality_score
```

**C-5.1** — The binary is located by `WAYBILL_SBOMQS_BIN` then `$PATH`, matching
`waybill-cli/tests/sbomqs_parity.rs:33`.
**C-5.2** — Unlike `sbomqs_parity.rs`, absence is **not** a silent skip here. That test may skip
because it is one signal among many; this command's entire purpose is the score.
