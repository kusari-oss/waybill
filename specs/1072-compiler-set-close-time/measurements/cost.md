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

## Derived, not measured — and later measured

Flagged separately because extrapolation is not measurement:

- **~478 MB/s** hashing throughput (955,494 bytes ÷ 2 ms). Two measured
  quantities divided, on one host, over a sample too small to resolve
  2 ms of timer granularity. Treat it as an order of magnitude, not a
  rate: the real figure is somewhere between roughly 100 MB/s and
  1 GB/s, and a 2 ms reading cannot narrow it further. Re-measure with a
  multi-hundred-MB fixture before relying on it for anything.

> **Superseded.** `budget-probe.sh` did that re-measurement over 7.1 s
> and 12.7 s samples: **~120 MB/s** (880,376,492 ÷ 7.131 s = 123 MB/s;
> 1,515,056,247 ÷ 12.734 s = 119 MB/s, same host). The derived figure was
> ~4× high. The caveat above was correct and the number was not, which is
> the reason the project rule puts predictions outside the data table.

## The default budget constant — original sizing, now superseded

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

> **The ~2,200× headroom in the first bullet is wrong by three orders of
> magnitude.** It is a fact about a four-crate fixture, which is the wrong
> denominator for a global default. The section below measures the right
> one and revises the constant to 16 GiB. The last paragraph survives
> intact, and the probe sharpened it: see "Why a higher budget does not
> endanger SC-008".

## Sizing the default against a real workspace

Produced by `budget-probe.sh`, committed next to this file. Same image,
same host, same kernel as above. Two traced builds of waybill itself, each
with a **fresh target dir** and the budget flag set to 64 GiB so the pass
hashes everything and reports true demand rather than clamping.

| traced build | wall-clock | paths_total | bytes hashed | pass | ratio | headroom at 2 GiB |
|---|---|---|---|---|---|---|
| `two_binaries_diverge` (above) | 0.9 s | 102 | 955,494 | 2 ms | 0.2% | ~2,200× |
| `--release --bin waybill` | 285.3 s | 6,537 | 880,376,492 | 7,131 ms | 2.4% | 2.4× |
| `--release --workspace --all-targets` | 685.0 s | 17,188 | 1,515,056,247 | 12,734 ms | 1.8% | **1.4×** |

The third row is the shape CI compiles.

**Both of the last two rows understate demand**, because this image puts
`CARGO_HOME` and `RUSTUP_HOME` under `/usr/local`, and m213's System
filter drops the `/usr/` prefix wholesale. Every read of a dependency's
sources (299 MB) and of the Rust sysroot (1.1 GB) was therefore invisible
to the pass. On an ordinary host those live at `~/.cargo` and `~/.rustup`,
which match **no** pattern in the catalog, so those reads do reach it.
`budget-probe.sh RELOCATE_TOOLCHAIN=1` measures that layout, and prints
both homes in its environment block so no future run's numbers can be
read without knowing which layout produced them.

Over all runs of the `--workspace --all-targets` build (see the paired
table below for the rest):

| toolchain layout | bytes demanded | % of the old 2 GiB |
|---|---|---|
| under `/usr/` — filtered, as this image ships | 1.39–1.52 GB | 65–71% |
| under `/root/` — visible, as an ordinary host | 1.77–2.00 GB | **82–93%** |

**Revised: 16 GiB.** ~8–9× the representative-layout demand, and still
64× the 256 MB per-file cap. At 82–93% of 2 GiB consumed by an ordinary
build of this repository, the old default was effectively already
breached, not merely approached: truncation costs real data and was
reserved for the pathological case.

A full-spend duration is deliberately **not** quoted here. See "Hashing
throughput is not a constant on this host".

### The capture is near-complete, so these are facts about the budget

A small `bytes_hashed` has two readings — an ample budget, or a trace that
saw a fraction of the build — and they call for opposite decisions. So the
probe records ground truth in the same run:

| | `--bin waybill` | `--workspace --all-targets` |
|---|---|---|
| bytes hashed | 880,376,492 | 1,515,056,247 |
| artifact bytes in the fresh target dir | 927,472,769 | 1,594,179,924 |
| hashed ÷ on-disk | **94.9%** | **95.0%** |
| invocations captured | 582 | 1,310 |
| with a non-empty write set | 527 | 1,247 |
| write-set entries | 3,432 | 7,012 |
| read-set entries | 21,048 | 96,013 |
| `events_dropped` / `ring_buffer_overflows` | 0 / 0 | 0 / 0 |

95% of the artifact bytes the build left on disk were hashed, with no ring
buffer loss. The remaining 5% is the `paths_absent` tail plus two paths
counted `paths_skipped` in each run — present at close but not hashable.
Those two were **not** inspected individually; the fixture's single
skipped path was a directory that reached a write set, and the same cause
is plausible here but unverified, so it is recorded as unexplained rather
than assumed.

### Hashing throughput is not a constant on this host

An earlier revision of this section quoted **~120 MB/s** from two samples
(123 and 119) and used it to bound a full budget spend at ~2.3 minutes.
More runs destroyed that figure:

| run | bytes hashed | pass | implied throughput |
|---|---|---|---|
| paired A #4 | 1,392,523,084 | 2,901 ms | 479 MB/s |
| paired A #1 | 1,515,039,978 | 3,484 ms | 435 MB/s |
| paired A #2 | 1,772,435,829 | 4,330 ms | 409 MB/s |
| `--all-targets`, relocated | 1,996,663,640 | 6,291 ms | 317 MB/s |
| `--bin waybill` | 880,376,492 | 7,131 ms | 123 MB/s |
| `--all-targets`, separate container | 1,515,056,247 | 12,734 ms | 119 MB/s |
| paired A #3 | 1,996,591,239 | 16,743 ms | 119 MB/s |
| paired B #1 | 1,953,456,369 | 17,239 ms | 113 MB/s |
| paired B #3 | 1,996,591,239 | 24,029 ms | 83 MB/s |
| paired B #2 | 1,996,591,335 | 24,812 ms | 80 MB/s |

**A 6× spread, same host, same image, several of them inside one
container.** Compare rows 4, 9 and 10: the same 1.9966 GB hashed in
6,291 ms, 24,029 ms and 24,812 ms — identical work, four times the time.
Wall-clock moved as much: 118.9, 175.1, 190.3, 204.8, 345.1, 524.7,
685.0, 685.4, 857.6 s for the same 392-crate build. This is a Colima VM
on an Apple-silicon host over virtiofs, and its I/O throughput is not
stable enough to support a single rate.

So no duration bound is quoted for a full budget spend. On this host a
16 GiB spend lands between ~35 seconds and ~3.4 minutes — too wide to
design against, and obtained on hardware nobody ships on. Measure it on
the target class before relying on it.

Worth naming the pattern rather than just fixing the number: the
~478 MB/s this file originally derived sits at the **top** of the
measured range, and the ~120 MB/s that replaced it sits near the
**bottom**. Both were single-point readings of a quantity that does not
hold still here. Quoting either as "the" throughput was the same mistake
twice, the second time while correcting the first.

What survives from the original note is the unquantified form of its
argument: hashable bytes and build duration scale together, because
artifacts cannot be produced faster than the build runs. The ratios
support it (see below), the rate does not pin it down, and the budget is
not what holds SC-008.

### Why a higher budget does not endanger SC-008

Raising the default from 2 to 16 GiB does not move the pass toward the
ceiling, because the pass only hashes what the trace observed — the
budget caps that, it does not create work. Six observations of the
`--workspace --all-targets` build, two layouts:

Nine observations are tabulated in findings.md Q7. **Two of them exceed
SC-008's 5% ceiling** — 5.2% and 7.1%.

An earlier revision of this section had six observations, read the single
5.2% as an outlier caused by the fastest wall-clock in the set, and
concluded the ceiling was safe. The 7.1% retired that: its wall-clock,
345 s, is mid-range.

What the nine runs show instead is that the criterion's *form* is the
problem. The pass costs **6.3–24.8 s** for ~2 GB hashed; the build it is
quoted against ranged **118.9–857.6 s for identical work** on this host.
So the ratio is breached precisely when the host is fast, with the pass
behaving the same or better. A machine compiling this workspace in three
minutes would read ~13%.

**The ceiling is not re-baselined here.** `spec.md` requires an overrun
to be recorded and the ceiling re-decided deliberately, and findings.md
Q7 carries the record, the three options and a recommendation
(re-express in absolute terms — seconds per GB hashed, measured at
3.2–12.4 s/GB — rather than as a fraction of wall-clock). That decision
needs a run on m669's reference class, which this file does not have.

What the budget is actually for is the case where that correlation breaks,
and the probe names it more precisely than the original note did: **it is
the read set, not the write set.** Artifact bytes cannot be produced
faster than the build runs, but read-set paths — rlibs, sysroot objects,
headers — existed before the trace started and their size is bounded by
nothing the trace can see. 96,013 read entries against 7,012 write
entries in the wider build is the shape of that exposure.

### The paired measurement

`budget-probe-paired.sh`, committed next to this file. Four
`--workspace --all-targets` builds inside **one** container, fresh target
dir each, in the order `F R R F` so each layout gets an early and a late
slot and host drift biases both arms rather than one.

| # | layout | wall (s) | pass (ms) | ratio | paths | hashed | bytes hashed | read entries |
|---|---|---|---|---|---|---|---|---|
| 1 | filtered | 204.820 | 3,484 | 1.7% | 17,188 | 2,769 | 1,515,039,978 | 96,057 |
| 2 | relocated | 175.096 | 4,330 | 2.4% | 19,141 | 8,190 | 1,772,435,829 | 137,601 |
| 3 | relocated | 524.688 | 16,743 | 3.1% | 23,200 | 8,602 | 1,996,591,239 | 184,371 |
| 4 | filtered | 190.313 | 2,901 | 1.5% | 15,029 | 2,553 | 1,392,523,084 | 81,093 |

Why it was run: two single runs had disagreed by 5.7× on wall-clock for
identical work, and SC-008's ratio is a quotient of exactly the two
numbers that moved. A ratio from unpaired runs on a host that swings that
far is not evidence either way.

What it settled: the **layout** effect is real and large (81k–96k read
entries filtered against 137k–184k relocated, with write entries pinned
at 7,012), and the **ratio** sits at 1.5–3.1% across both arms.

What it did not settle, and a claim to withdraw: paths_total varied
**15,029 → 23,200** and bytes_hashed **1.39 → 2.00 GB** between runs of
one build. On the strength of rows 1 and 3 — which reproduced the
separate-container runs to within 0.001% and 0.004% — an earlier note in
this file called the filtered arm "reproducible to 16 KB". Row 4, 8%
below, retired that. Demand is a range, not a point, and the ranges in
the sizing table above are stated accordingly.

The capture variance was the open question this table could not answer:
it carries no `events_dropped` or `ring_buffer_overflows`, so whether the
variance was **disclosed loss** (the system working as designed,
Principle X) or **silent non-determinism** (a defect) was not decidable
from these rows. The script now prints both counters per run, and a
second set of three identical runs answered it: **silent.** One run saw
50 fewer compiler invocations and 753 fewer paths than the other two with
every integrity counter reading zero and `completeness: complete`. That
is an m210/m212 finding, not a close-time one, and it is the most
consequential thing these probes turned up — findings.md Q7 has the
table, the consequences for #1142, and what was deliberately not
investigated.

### Margins, stated plainly

Both of the original claims passed, with far less room than the fixture
suggested:

| claim | fixture says | real workspace says |
|---|---|---|
| SC-008 ≤ 5% of wall-clock | 0.2% (25× inside) | 1.5–3.1%, one 5.2% outlier |
| budget headroom | ~2,200× | 1.1–1.2× at 2 GiB, ~8–9× at 16 GiB |

### Re-running

```sh
docker run --rm --privileged \
  -v /sys/kernel/debug:/sys/kernel/debug \
  -v "$PWD/specs/1072-compiler-set-close-time/measurements/budget-probe.sh:/budget-probe.sh" \
  -v "$PWD/specs/1072-compiler-set-close-time/measurements/.probe-out:/probe-out" \
  -e BUILD_ARGS="--release --workspace --all-targets" \
  -e TARGET_DIR=/waybill/target-m1072-probe-all \
  -e RELOCATE_TOOLCHAIN=1 \
  --entrypoint bash waybill-ebpf-test /budget-probe.sh
```

Drop `RELOCATE_TOOLCHAIN=1` to measure the filtered layout instead. For
the paired form — several builds alternating layout inside one container,
which is the only way the ratio means anything on this host:

```sh
docker run --rm --privileged \
  -v /sys/kernel/debug:/sys/kernel/debug \
  -v "$PWD/specs/1072-compiler-set-close-time/measurements/budget-probe-paired.sh:/paired.sh" \
  -e SEQUENCE="F R R F" \
  --entrypoint bash waybill-ebpf-test /paired.sh
```

**The target dir must not be under `/tmp`.** The first two runs of this
probe put it there and measured a demand of 9.97 MB with *zero* write-set
entries across a 358-crate build, because m213's kernel noise filter drops
the `/tmp/` prefix as category `Ephemeral`
(`waybill-common/src/filter.rs:139`). That reads exactly like a capture
defect and is an experiment artifact. `budget-probe.sh` now refuses such a
path with exit 2 rather than reporting the number. findings.md Q6 records
it.

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
