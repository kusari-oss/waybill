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
   jq '.metadata | {runner_uname, noise_class}' docs/perf/baseline.json
   # noise_class MUST be "reference"
   ```

5. Regenerate the numbers page and commit both:

   ```bash
   cargo run -p xtask -- bench-docs
   ```

## When to refresh

After a release, or after a change that legitimately shifts the
performance envelope — the rayon thread-pool work in #732/#734/#739
added ~2 MB of fixed memory, which is a real and expected shift.

Refreshing to silence a regression you have not explained is how a
performance gate stops being one. Explain the shift first, in the PR
body; then refresh.
