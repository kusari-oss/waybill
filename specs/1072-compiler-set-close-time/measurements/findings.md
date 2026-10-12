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

## Not measured

The close-time hashing cost that SC-008 ties its ceiling to. That
requires a real kernel trace with the pass implemented, so it belongs
to the first implementation task, not to this probe.
