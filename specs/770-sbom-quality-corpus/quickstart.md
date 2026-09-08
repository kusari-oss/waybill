# Quickstart: SBOM Quality Regression Corpus

## Prerequisites

```bash
# waybill release build — the binary under measurement
cargo build --release -p waybill --bin waybill

# sbomqs — the external quality scorer. Pin the version the corpus expects.
GOMODCACHE="$(mktemp -d)" go install github.com/interlynk-io/sbomqs/v2@v2.0.6
export PATH="$HOME/go/bin:$PATH"
```

`git` is assumed present. First run fetches ~2.2 GB across 18 repositories in roughly 95
seconds; subsequent runs reuse `~/.cache/waybill/quality-corpus/`.

## Measure everything

```bash
cargo run -p xtask --release -- quality
```

Prints a per-target table, writes `target/quality/run-<sha12>.json`, and exits non-zero if any
measurement is outside its authored range.

## Iterate on one target

```bash
cargo run -p xtask --release -- quality --filter 'gradle-*'
cargo run -p xtask --release -- quality --filter go-kubernetes
```

Globs union across repeated flags. An empty selection is not an error.

## Author ranges for a new repository

1. Add the target to `xtask/corpus/quality-corpus.toml` with **no** `[targets.expect]` block:

   ```toml
   [[targets]]
   name      = "my-new-target"
   url       = "https://github.com/org/repo"
   sha       = "<40 hex — resolve with: git ls-remote --tags --refs <url>>"
   ecosystem = "npm"
   ```

2. Measure it without gating:

   ```bash
   cargo run -p xtask --release -- quality --filter my-new-target --no-gate
   ```

3. Copy the observed values into a comment, then author bounds around them:

   ```toml
   # Observed 2026-09-03 (offline):
   #   wall=218ms sbomqs=7.66 pkgs=620 files=22 edges=890 depth=7 flat=false
   #   waybill self-report: partial
   [targets.expect]
   pkgs   = { min = 580, max = 660 }
   sbomqs = { min = 7.2, max = 8.1 }
   flat   = false
   ```

   Leave out any measurement you do not want to gate — absent means observe-only.

4. Re-run without `--no-gate` and confirm it passes.

### Authoring guidance

- **`sbomqs`** sits in a narrow 5.75–7.70 band corpus-wide. ±0.5 is meaningful; a percentage
  band is not.
- **`wall_ms`** is a single sample on shared hardware. Author order-of-magnitude guards that
  catch a collapse, not tight performance assertions.
- **`pkgs` and `files` move independently.** Some targets are mostly file-tier (ansible: 11
  packages, 375 files). Bound them separately.
- **`flat = true` is legitimate.** Several upstreams commit no lockfile and are permanently
  flat. Assert what is true, not what you wish were true.

## Reading a failure

```text
VIOLATIONS (2)
  pnpm-vue-core   pkgs   expected 580..660, observed 412
  rust-zizmor     flat   expected false, observed true
```

Each line names the target, the measurement, the bound, and the observation — enough to act on
without re-running. The JSON report carries the same data plus every passing measurement.

Exit codes: `0` clean, `1` violations or unmeasurable targets, `2` broken corpus file.

## Gotchas

- **The numbers are waybill's *offline* floor.** Scans run `--offline` for reproducibility, so
  improvements that only appear with network resolution are invisible here. See research R2.
- **`graph_completeness` never gates.** It is recorded beside the independently measured
  flatness precisely so the two can be compared — three targets currently self-report
  `complete` while measuring flat.
- **pytorch's `third_party/` is empty on purpose.** Sub-repositories are not fetched; its
  counts are lower than a working copy's and that is expected.
- **A missing `sbomqs` fails the run**, even under `--no-gate`. A missing signal is not a
  passing signal.
