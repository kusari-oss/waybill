# Milestone 1072 measurements (#1144)

Run `./probe.sh` to reproduce. Probes 1 and 4 read milestone 1070's
committed strace of a real kernel trace (Colima aarch64, Ubuntu 24.04,
kernel 6.8.0-136); probes 2 and 3 run on any host, because inode identity
and file survival need no eBPF. Probe numbering follows the order the
probes were written, not the questions: probe 1 → Q1, probes 2–3 → Q2,
probe 4 → Q3. Output below captured 2026-10-11 (probes 1–3) and
2026-10-12 (probe 4) on macOS (darwin 25.5.0) against
`two_binaries_diverge` (2 lib crates, 2 bins).

## Q1. Can one path appear in two invocations' write sets?

**No**, and it is structurally prevented. Do not specify for it.

| probe | result |
|-------|--------|
| 1a parse coverage | `raw=70 parsed=70` — every write-mode open parsed |
| 1b real multi-writer paths | `/dev/fd/5` only, by pids 154/155/174/175 |
| 1c positive control | injected writer **surfaced** |

`/dev/fd/5` is a pipe, not an artifact. No real path is opened for
writing by more than one pid.

Probe 1c matters: a zero result in 1b is indistinguishable from a
broken `sed`, so the control re-emits one real artifact open under a
fake pid and confirms the detector fires. It does.

**Why it is structural, not luck.** Cargo embeds a metadata hash in
every output filename (`liblibsafe-603d5d27712eef2e.rlib`,
`safe_only-049f485b9f94a33f`) and compiles each
(crate, feature-set, target) exactly once per build. Two invocations
writing one final path would require two compilations sharing a
metadata hash, which is the thing fingerprinting exists to prevent.

**Limit of this evidence.** It covers one fixture. Milestone 1070 also
straced a larger build (`-p waybill-common`: 31 rustc, 12 cc, 8 build
scripts) and recorded the same write patterns in `strace_summary.txt`,
but `common.strace` itself was not committed, so the larger build was
not re-analysed here. Build scripts writing a shared path in `OUT_DIR`
remain the one unmeasured route to a multi-writer path; they are not
compiler invocations, so they do not reach a `write_set`.

## Q2. How do compiler outputs actually reach their final names?

Three distinct mechanisms, from `strace_summary.txt` plus probes 2–3:

- **Binaries** — the linker opens `deps/<bin>-<hash>` directly with
  `O_RDWR|O_CREAT|O_TRUNC`. No rename.
- **Libraries** — rustc writes a randomized temporary and `renameat()`s
  it into place; *the final library path is never opened for writing*:
  - `deps/rmeta*/full.rmeta` → `deps/lib<crate>-<hash>.rmeta`
  - `deps/.tmp*.temp-archive/tmp.a` → `deps/lib<crate>-<hash>.rlib`
- **User-facing name** — cargo materialises `release/<bin>` from
  `deps/<bin>-<hash>` *after* compilation. Mechanism is
  platform-dependent: `linkat` on Linux (m1070), a copy on macOS
  (probe 2: `ino=559840360` vs `ino=559840356`, both `links=1`,
  identical content `ba4d0a47…`).

So `two_binaries_diverge` exercises both the direct-open and the
rename-into-place paths, and is an adequate fixture for US1 and US3.

Probe 3 confirms the rustc temporaries are gone by trace end
(`deps/rmeta*` does not exist), which is exactly the case #1144
reports as falsely `survived_trace_window: true`.

## Q3. Can one path appear in two invocations' *read* sets?

**Yes, routinely.** This is the mirror of Q1 and the opposite answer.

Probe 4 walks each reader pid up the `clone()` edges to the `rustc` that
owns it, because `strace -f` lists worker threads as separate pids — without
that walk, one rustc's threads look like several invocations.

`deps/liblibsafe-2d5f1ecd358a29fb.rlib` is read under **three distinct
rustc invocations**:

| reader pid | owning invocation |
|-----------|-------------------|
| 157 | rustc 154 |
| 177 | rustc 174 |
| 180 | rustc 175 |
| 152 | cargo (pid 139), not a compiler invocation |
| 196, 197 | `/usr/bin/ld`, linker children |

The 7 rustc invocations in the trace are pids 142 144 149 154 155 174 175.
Both fixture binaries declare `libsafe = { path = ... }`, so its `.rlib` is
read by both of their compilations plus the lib's own, which is exactly the
shape expected. Across the whole trace, **213 paths have more than one
reader.**

**Consequence.** The close-time pass must count and charge per *distinct
path*, not per set entry. Charging per entry would bill one `.rlib`'s bytes
three times against the budget, and would let two entries for one path
disagree about its close-time state if the file changed between their
checks. Spec: FR-011, FR-011a, FR-013, SC-010.

Note the asymmetry this creates with Q1: a write path has exactly one
producing invocation, so `sha256_at_close` is attributable; a read path has
many consuming invocations, so the same hash is legitimately shared by all
of their entries.

## Consequences for the spec

1. No multi-writer FR. Recorded as an assumption with this evidence.
2. A hard link is **not** a rename: both names survive and share an
   inode. `renamed_from` is for renames only.
3. Two write-set paths may legitimately share an inode and hash
   identically. Not an error.
4. `release/<bin>` — the artifact an operator ships — appears in **no**
   compiler write set, because no compiler wrote it. A real provenance
   gap, but outside #1144; needs its own issue.
5. Hash and stat **once per distinct path** per trace, with the coverage
   counts over distinct paths (Q3).

## If something here is later found wrong

This file is also where negative results belong. The tracing subsystem is
experimental, so an approach recorded above may turn out not to work —
append the finding and what it cost rather than deleting the claim it
replaces. A measurement that was superseded still tells the next person
which question had already been asked.

## Not measured

The close-time hashing cost that SC-008 ties its ceiling to. That
requires a real kernel trace with the pass implemented, so it belongs
to the first implementation task, not to this probe.

## Q5. Can two traces of one build produce byte-identical payloads?

**No, and this predates milestone 1072.** Measured while implementing
T043, which originally asked the harness to assert exactly that.

Two consecutive `trace capture` runs of `two_binaries_diverge`, with
`target/` removed between them, on Colima 6.8.0 aarch64. The normalised
payloads differ in two places, both in the **observation** tier that
Principle II forbids the close-time pass to touch:

```
17c17
<           "invocation_id": 3114349875006
>           "invocation_id": 3118339958275
...
151c151
<   ".../target/release/deps/rmetaA36mif"
>   ".../target/release/deps/rmeta35kQFf"
156c156
<   ".../target/release/deps/rustceARhX8"
>   ".../target/release/deps/rustcdRlOq7"
194c194
<   ".../deps/.tmpZ6GhkV.temp-archive/tmp.a"
>   ".../deps/.tmpCc2MIw.temp-archive/tmp.a"
```

1. **`invocation_id` is `bpf_ktime_get_ns()` at exec time.** Milestone
   210's deliberate choice, commented at
   `waybill-cli/src/trace/compiler_pipeline.rs:371` — "unique +
   monotonically increasing within a boot". A boot-relative nanosecond
   clock cannot repeat across runs, so no two traces have ever agreed
   here.
2. **rustc's temporaries are `mkdtemp`-random.** `rmeta<6 random>`,
   `rustc<6 random>` and `.tmp<6 random>.temp-archive` appear in read
   sets, write sets and `renamed_from` because the kernel genuinely
   observed them. m1070 shipped those sets; the randomness arrived with
   them.

### Why the original requirement looked safe

It cited feature 006 US2. That text reads "byte-identical across re-runs
**when the build is reproducible**", and FR-006 constrains the *signing
step*: re-signing one canonical payload must not change the payload
bytes. Neither promises that two *traces* agree. The conflation is
between a build whose **outputs** are reproducible — this fixture's are —
and a build whose **process** is. The trace records the process.

### Disposition

C-1072-6 now requires what the pass can actually deliver: given the same
observation tier and budget, the close-time tier and the coverage record
are identical. SC-005a states it, and T043's harness section asserts it
by comparing the coverage record plus every path's close-time facts over
the paths both runs observed — an intersection, because the random
temporaries differ by name and never meet.

Evidence that the narrower claim holds: `cost.md`'s five runs produced
`paths_total=102, paths_hashed=14, bytes_hashed=955494` with no column
moving, and the corrected harness assertion reports the coverage record
identical across runs with identical close-time facts for every shared
path.

### How much of the set is per-run

**50 of 102 paths appear in both runs.** The other 52 — just over half —
are `mkdtemp` names that exist in exactly one trace. That ratio is the
scale of the problem the old requirement would have had to solve: not a
field or two, but half the path set. It is also why the harness compares
an intersection and then asserts a floor on its size: 50 shared paths is
a real check, whereas a handful would have been close to vacuous.

**Recorded rather than quietly relaxed**, per `spec.md`'s experimental
posture: the subsystem is pre-stable, and an obligation found to be
unachievable belongs in this file with its measurement, not in a silent
edit.

### Open consequence for signing

If byte-identical re-runs are ever genuinely needed — reproducible-build
verification comparing two independently produced attestations — it is
`invocation_id` that has to change, not this pass: a content-derived
identifier (hash of argv + cwd + ordinal) rather than a clock. That is a
predicate-shape change well outside #1144, and it would not fix the
`mkdtemp` paths, which are irreducibly per-run facts about what happened.

## Q6. Does the 2 GiB default budget survive a real workspace?

**No — it was 1.4× a build this repo's own CI performs.** Measured after
T049 was marked done, by `budget-probe.sh`, because the question "where
did 2 GiB come from" has a worse answer than it looked.

`cost.md`'s original sizing cited the measurement it was required to
cite, and that measurement was of `two_binaries_diverge`: four crates,
955,494 bytes. The headroom was quoted as ~2,200×. A four-crate fixture
is the wrong denominator for a global default, and the project rule the
spec cites — measure external behaviour before designing around it —
applies to the scale of a real build just as much as to an API's page
size. 2 GiB satisfied the letter of C-1072-9 while resting on a toy.

Two traced builds of waybill itself, fresh target dir, budget flag at
64 GiB so nothing clamps:

| traced build | bytes demanded | % of 2 GiB |
|---|---|---|
| `two_binaries_diverge` | 955,494 | 0.04% |
| `--release --bin waybill` | 880,376,492 | 41.0% |
| `--release --workspace --all-targets` | 1,515,056,247 | **70.5%** |

Truncation costs real data, so the default is meant to be reached only
by something pathological. At 70.5% consumed by an ordinary CI build,
any workspace 1.5× this one hit it. **Revised to 16 GiB** (~11× the
CI-shaped demand); full detail and the SC-008 consequences are in
`cost.md`.

Two of `cost.md`'s numbers were also measured properly for the first
time: hashing throughput is **~120 MB/s**, not the ~478 MB/s derived
from a 2 ms sample, and the pass costs **1.8–2.4%** of a real build's
wall-clock rather than 0.2%. Both original figures were labelled
honestly — the throughput as derived, the ratio as fixture-specific —
and both were wrong by about 4× and 10× respectively. A correct label on
a wrong number still ships the wrong number.

### The probe measured a kernel filter twice before it measured a budget

Worth recording because the failure was indistinguishable from a product
defect. The first two runs put the fresh target dir at
`/tmp/m1072-probe-target` and reported:

```
paths_total=496 paths_hashed=419 bytes_hashed=9971025
total write entries: 0        (across 582 invocations, 358 crates)
```

Zero write-set entries for an entire release build, with
`events_dropped=0` and `completeness: complete`. That reads as a capture
defect in m210/m213 — and it is m213 working exactly as designed:
`Ephemeral` filters the `/tmp/` prefix in-kernel
(`waybill-common/src/filter.rs:139`), so every artifact write was dropped
before userspace saw it. The same build with the target dir under
`/waybill/` produced 3,432 write entries and 839 MiB.

Three things made it recoverable, all of them cross-checks the first
version of the probe did not have:

1. **cargo's own `Compiling` count** as ground truth for how much work
   the build did;
2. **artifact bytes left in the fresh target dir**, which bounds what a
   complete capture would have faced — 884 MiB against 9.97 MB hashed,
   i.e. 1%, is not a plausible reading of an ample budget;
3. **write-set entry count**, asked directly, because #1144 exists to
   resolve write-set entries and zero of them means the probe measured
   nothing about its own subject.

`budget-probe.sh` now refuses a `/tmp` or `/var/tmp` target dir with exit
2 rather than printing a number, and prints all three cross-checks beside
the result. The lesson is the one already in the project's memory in a
different costume: a number that looks like "plenty of headroom" and a
number that means "the experiment measured nothing" are the same number,
so the probe has to carry its own positive control.
