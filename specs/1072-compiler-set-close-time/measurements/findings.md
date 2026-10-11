# Milestone 1072 measurements (#1144)

Run `./probe.sh` to reproduce. Probe 1 reads milestone 1070's committed
strace of a real kernel trace (Colima aarch64, Ubuntu 24.04, kernel
6.8.0-136); probes 2 and 3 run on any host, because inode identity and
file survival need no eBPF. Output below captured 2026-10-11 on macOS
(darwin 25.5.0) against `two_binaries_diverge` (2 lib crates, 2 bins).

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

## Q3. Consequences for the spec

1. No multi-writer FR. Recorded as an assumption with this evidence.
2. A hard link is **not** a rename: both names survive and share an
   inode. `renamed_from` is for renames only.
3. Two write-set paths may legitimately share an inode and hash
   identically. Not an error.
4. `release/<bin>` — the artifact an operator ships — appears in **no**
   compiler write set, because no compiler wrote it. A real provenance
   gap, but outside #1144; needs its own issue.

## Not measured

The close-time hashing cost that SC-008 ties its ceiling to. That
requires a real kernel trace with the pass implemented, so it belongs
to the first implementation task, not to this probe.
