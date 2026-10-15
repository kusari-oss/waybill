# Refreshing the performance baseline

`docs/perf/baseline.json` is what `bench.yml` compares every run
against. It fails closed: any dimension crossing the 25% threshold
exits non-zero.

## Take the baseline from CI, never from a laptop

Wall-clock and peak-RSS do not transfer between host classes. A
baseline recorded anywhere but a reference-class runner turns every
subsequent comparison into a measurement of the two machines rather
than of the code.

This is not hypothetical. The baseline committed in
[#741](https://github.com/kusari-oss/waybill/pull/741) was recorded on
an arm64 macOS laptop (`noise_class: "noisy"`) while `bench.yml` runs
on `ubuntu-latest`. Every bench run for the next nine days failed —
six runs, including five release tags — reporting 65 phantom
regressions with `MaxRssKb` deltas of +300% to +2800%. The code was
fine throughout.

`xtask bench` now refuses to compare across noise classes, so this
fails with one clear sentence instead of a table of fiction. But the
refusal only tells you the baseline is wrong; the recipe below is how
you get a right one.

## Recipe

1. Dispatch the `bench` workflow on `main`:

   ```bash
   gh workflow run bench.yml --repo kusari-oss/waybill --ref main
   ```

2. Wait for it, then download the artifact. It uploads on failure too
   (`if: always()`), so a run that fails the comparison against a stale
   baseline still produces a usable new one:

   ```bash
   gh run download <run-id> --repo kusari-oss/waybill --dir /tmp/bench
   ```

3. Copy the run JSON over the baseline:

   ```bash
   cp /tmp/bench/bench-run-*/run-*.json docs/perf/baseline.json
   ```

4. Confirm it is reference-class before committing — this is the check
   that was missed:

   ```bash
   jq '.metadata | {runner_uname, noise_class, rustc_version}' docs/perf/baseline.json
   # noise_class MUST be "reference"
   # rustc_version SHOULD be present and match rust-toolchain.toml
   ```

5. Regenerate the numbers page and commit both:

   ```bash
   cargo run -p xtask -- bench-docs
   ```

## The compiler is recorded too, and a pin bump is a refresh trigger

`metadata.rustc_version` holds `rustc --version` as it resolved when the
baseline was captured (#1195). Compiler version is a first-order input to
both wall-clock and peak-RSS — codegen, inlining and MIR opts all move
between releases — and it was the one invalidating input the block did
not record. `rust-toolchain.toml`'s own header says a compiler change
"invalidates every artifact by design"; the baseline's numbers are such
an artifact.

So **bumping the pin in `rust-toolchain.toml` is a reason to refresh**,
alongside the two below. `xtask bench` prints a note to stderr when the
baseline's compiler differs from the running one, or when the baseline
records none — a note, not a refusal, because comparing across a pin
bump is often exactly what you want to see. It only has to not look like
a code regression.

Baselines captured before #1195 have no `rustc_version` at all, and the
note says so rather than treating the silence as agreement. The one
committed on 2026-09-08 is in that state; it will carry a compiler from
its next refresh onward.

## When to refresh

After a release, after a `rust-toolchain.toml` pin bump, or after a
change that legitimately shifts the performance envelope — the rayon
thread-pool work in #732/#734/#739 added ~2 MB of fixed memory, which is
a real and expected shift.

Refreshing to silence a regression you have not explained is how a
performance gate stops being one. Explain the shift first, in the PR
body; then refresh.
