# Close-time pass cost — milestone 1072 (#1144)

Tasks T034 (baseline) and T035 (the pass itself). Produced by `cost.sh`,
which is committed next to this file so the finding is reproducible and so
the next person can re-run it when the subsystem changes.

These are the two numbers `contracts/cli-budget.md` C-1072-9 and SC-008
refuse to let the spec guess. SC-008's **5% ceiling was fixed in `spec.md`
before this measurement ran** — a ceiling the measurement sets cannot be
missed.

## Environment

- kernel: `Linux 6.8.0-136-generic`
- arch: `aarch64` (Colima VM on an Apple-silicon host, 8 vCPU / 12 GiB)
- fixture: `waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge`
- waybill: `0.10.1`, release build, `--features ebpf-tracing`
- commit: `e62bc8ac` (recorded by hand — `.dockerignore` excludes `.git/`,
  so `cost.sh` cannot read it from inside the container and prints
  `not a git checkout`)
- command: `waybill trace capture --attestation-format waybill-v1`
- `$FIXTURE/target` removed before every run, so no run measures a no-op
  build

## Measured

5 runs:

| run | trace wall-clock (s) | pass (ms) | ratio | paths | hashed | bytes hashed |
|-----|---------------------|-----------|-------|-------|--------|--------------|
| 1 | 0.894 | 2 | 0.2% | 102 | 14 | 955494 |
| 2 | 0.927 | 2 | 0.2% | 102 | 14 | 955494 |
| 3 | 0.879 | 2 | 0.2% | 102 | 14 | 955494 |
| 4 | 0.954 | 2 | 0.2% | 102 | 14 | 955494 |
| 5 | 0.904 | 2 | 0.2% | 102 | 14 | 955494 |

**Mean ratio: 0.2% of total trace wall-clock. SC-008's ceiling is 5%, so
the pass is ~25× inside it.**

### What the identical columns mean

`paths`, `hashed` and `bytes hashed` are byte-identical across all five
runs, and `pass (ms)` does not move. That is not a rounding artifact — it
is the determinism C-1072-6 requires, observed rather than asserted: the
pass visits a sorted path set and charges the budget in that order, so the
same build produces the same work. Had any column wobbled, SC-005 would
have been in doubt before the determinism harness assertion (T043) ever
ran.

### Where the 102 paths go

- `paths_hashed = 14` — the artifacts still on disk at trace end.
- `paths_absent = 87` — overwhelmingly rustc's own temporaries: `rmeta`
  staging directories, intermediate object files, and the renamed-away
  sources of the 4 rename chains. **This is #1144 in one number.** Before
  this milestone all 87 reported `survived_trace_window: true` with a
  64-zero hash.
- `paths_skipped = 1` — `target/release/deps`, a directory that reached a
  write set because the compiler opened it. Present, not a regular file,
  so not hashable.

87 of 102 paths being gone by trace end is also why SC-001 demands at
least one `existed_at_close: false` rather than merely "no
contradictions": the common case is absence, so a pass that silently did
nothing would be reporting the exact opposite of the truth for 85% of the
set.

## Derived, not measured

Flagged separately because extrapolation is not measurement:

- **~478 MB/s** hashing throughput (955,494 bytes ÷ 2 ms). Two measured
  quantities divided, on one host, over a sample too small to resolve
  2 ms of timer granularity. Treat it as an order of magnitude, not a
  rate: the real figure is somewhere between roughly 100 MB/s and
  1 GB/s, and a 2 ms reading cannot narrow it further. Re-measure with a
  multi-hundred-MB fixture before relying on it for anything.

## The default budget constant

**2 GiB** (`DEFAULT_COMPILER_HASH_BUDGET_BYTES`), with this reasoning:

- the fixture demands 0.93 MB, so 2 GiB is ~2,200× the measured need and
  a normal build will never see truncation. Truncation costs real data,
  so the default should be reached only by something pathological;
- at the derived throughput above it bounds the pass in the low seconds
  even when fully spent — and because that throughput is derived from a
  2 ms sample, the bound is a rough one, which is the honest claim;
- it is far above the 256 MB per-file cap, so the two limits do not
  interact in the normal case: a single enormous artifact is refused by
  the cap long before the cumulative budget notices.

**The budget does not enforce SC-008's ratio, and cannot.** A 2 GiB spend
against this fixture's 0.9 s trace would be ~500%, not 0.2%. The ratio
holds because hashable bytes and build duration scale together — a build
producing gigabytes of artifacts takes minutes, not a second. The budget
exists to bound the pathological case where that correlation breaks, and
`truncated: true` plus the FR-015 WARN is what an operator sees when it
does. Principle X: disclose, rather than claim a guarantee that cannot be
made.

## Re-running

```sh
docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .
docker run --rm --privileged \
  -v /sys/kernel/debug:/sys/kernel/debug \
  -v "$PWD/specs/1072-compiler-set-close-time/measurements/cost.sh:/cost.sh" \
  -e RUNS=5 \
  --entrypoint bash waybill-ebpf-test /cost.sh
```

The script is bind-mounted rather than baked in, so iterating on it costs
nothing — editing it inside the build context would invalidate the
cargo-build layer. See `quickstart.md` for why, and for the two other
container gotchas.

The pass's own summary is a DEBUG line, not INFO, so that measuring it
needs no code change while leaving default operator output unchanged
(C-1072-11). `cost.sh` strips ANSI before parsing it:
`tracing_subscriber::fmt` colourises even when stdout is a file, so
`grep paths_total=` finds nothing against the raw bytes. That cost one
run which correctly reported **ABSENT** in every row rather than a zero —
the script refuses to print a mean it could not measure.

## T047 — the quickstart verification block, run

Every check `quickstart.md` describes is now committed as a section of
`scripts/ebpf-integration-test.sh` and runs under `--privileged`. Measured
output of the full harness on Colima 6.8.0 aarch64:

```
==> m1072 US1: close-time facts (SC-001, SC-002, SC-003)
    write-set entries reporting absent at close: 14
    entries claiming resolution, re-checked on disk: 15
    all-zero hashes: 0
    hashes verified against sha256sum: 10/10 agree

==> m1072 US2: read-set honesty (FR-001)
    entries carrying a bare sha256 key: 0
    read-set hashes verified against sha256sum: 10/10 agree
    entries: 196 over 102 distinct paths, all agreeing

==> m1072 US3: rename history (SC-004)
    write-set entries carrying rename history: 4
    intermediate names shadowing a real entry: 0

==> m1072 US4: coverage record (FR-013, FR-014)
    {"paths_total":102,"paths_hashed":14,"paths_absent":87,
     "paths_skipped":1,"truncated":false,"budget_source":"default"}
    paths_total agrees with the sets: 102 distinct paths
    paths_hashed agrees with the entries carrying a hash: 14

==> m1072 US4: budget truncation (SC-006)
    exit 0, truncated: true, paths_hashed: 0, budget_source: operator
    existence still reported: paths_absent=87

==> m1072 US4: close-time determinism (SC-005a)
    coverage record identical across runs
    close-time facts identical for all 50 paths both runs observed

==> m1072 SC-007: zero-compiler trace is byte-unchanged
    trace_integrity keys: ["bloom_filter_capacity",
      "bloom_filter_false_positive_rate","events_dropped",
      "filter_categories_applied","kprobe_attach_failures",
      "partial_captures","ring_buffer_overflows","uprobe_attach_failures"]
    all present in the pre-m1072 struct; no coverage record emitted

>>> m210 + m212 + m213 + m1072 integration test PASSED
```

The quickstart's determinism snippet was **wrong as written** and is
corrected there: it diffed whole payloads. See findings.md Q5.

## T049 — what each lane actually verifies, and where

| lane | where it can run | result |
|------|------------------|--------|
| privileged harness | this image, `--privileged` + debugfs mount | PASSED |
| `clippy --all-targets --features ebpf-tracing -- -D warnings` | this image **with `tests/fixtures` bind-mounted** | exit 0 |
| `cargo test --workspace --features ebpf-tracing` | **a full checkout**, i.e. CI's runner — not this image | see below |

The image cannot run the full feature-on test lane, because
`.dockerignore` excludes `tests/fixtures/`, `docs/` and `specs/`, and the
image has no `spdx3-validate`. That is a property of the image, which
exists to be the integration harness, not a regression — and CI runs that
lane directly on `ubuntu-latest` with a complete checkout
(`ci.yml`), not in the container.

What the image can still establish about that lane, and did:

- all 15 `trace::close_time` tests and all 7 rename tests pass with
  `--features ebpf-tracing` on;
- **no test fails by attempting to attach eBPF unprivileged** — a grep of
  the whole run for `panicked.*(attach|ebpf|permission)` returns nothing.
  That is SC-009's actual concern and the reason Constitution VII wants a
  runtime privilege check rather than a `cfg` gate;
- mounting the excluded directories back in accounts for every other
  failure: `tests/fixtures` alone takes the run from a build error to 2061
  passing, `docs` adds the four `parity::catalog` tests, and what remains
  is the `spdx3-validate` family failing with
  `No such file or directory` on a binary the image does not carry.

So the feature-on lane's verdict belongs to CI. The claim made here is the
narrower one that can be measured locally.
