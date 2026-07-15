# Quickstart: Versionless PURL Round-Trip Fuzz Test

**Date**: 2026-07-15
**Audience**: mikebom maintainer implementing or reviewing m198.

## Prerequisites

- Working mikebom checkout on branch `198-purl-fuzz-test`.
- `cargo +stable` toolchain (existing workspace toolchain — no changes needed).

## Reproducer 1 — Run the fuzz suite

```bash
cargo test -p mikebom-common versionless_purl_fuzz -- --nocapture
```

**Expected on green m198**:
```
running 1 test
[versionless-purl-fuzz] npm: 120
[versionless-purl-fuzz] cargo: 120
[versionless-purl-fuzz] maven: 120
[versionless-purl-fuzz] gem: 120
[versionless-purl-fuzz] pypi: 120
[versionless-purl-fuzz] composer: 120
[versionless-purl-fuzz] pub: 120
[versionless-purl-fuzz] cocoapods: 120
[versionless-purl-fuzz] hackage: 120
[versionless-purl-fuzz] hex: 120
[versionless-purl-fuzz] scala: 120
test versionless_purl_fuzz_all_ecosystems ... ok

test result: ok. 1 passed; 0 failed
```

Total invocations ≥ 1100 per FR-002.

## Reproducer 2 — Verify SC-003 (fuzz catches a Purl regression)

Simulates a maintainer accidentally breaking `Purl::as_str()`:

```bash
# Introduce a regression — e.g., truncate the canonical string:
sed -i.bak 's|&self.canonical|\&self.canonical[..self.canonical.len()-1]|' \
  mikebom-common/src/types/purl.rs

# Run the fuzz — expect failure:
cargo test -p mikebom-common versionless_purl_fuzz -- --nocapture 2>&1 | tail -20

# Restore:
mv mikebom-common/src/types/purl.rs.bak mikebom-common/src/types/purl.rs
```

**Expected**: at least one assertion fires with a diagnostic block naming the ecosystem + shape + input + observed vs expected. Any of the 1100+ invocations may trip first; the exact one doesn't matter — as long as the count is > 0.

## Reproducer 3 — Verify SC-004 (pre-PR wall-clock delta ≤ 5s)

```bash
# 1. Time post-m198 (current HEAD):
time ./scripts/pre-pr.sh 2>&1 | tail -3

# 2. Stash m198 changes and time the pre-m198 baseline:
git stash push -m 'm198-scratch: measure pre-PR baseline'
time ./scripts/pre-pr.sh 2>&1 | tail -3
git stash pop
```

Delta MUST be ≤ 5s per SC-004. Expected delta is well under 1s (the fuzz test's 1100 invocations at microseconds each is trivially fast; the visible cost is cargo-test binary discovery).

## Reproducer 4 — Filter to a single ecosystem for iteration

If the maintainer is debugging drift in one ecosystem (say composer):

```bash
cargo test -p mikebom-common versionless_purl_fuzz_composer -- --nocapture
```

(Requires per-ecosystem `#[test]` decomposition — plan phase decision: either one big `#[test] fn versionless_purl_fuzz_all_ecosystems()` or eleven per-ecosystem `#[test] fn versionless_purl_fuzz_<ecosystem>()`. Recommend the per-ecosystem shape so cargo-test filter works per Reproducer 4.)

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) and `cargo +stable test --workspace` MUST pass green. Fuzz test runs in the default test lane per FR-006 — no opt-in gate.
