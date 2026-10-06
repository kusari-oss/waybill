# Research: Trace captures relative source reads and compiler writes

All measurements: Colima aarch64, Ubuntu 24.04, kernel 6.8.0-136, 2 CPUs, `Dockerfile.ebpf-test` built from `main` at `617498a5`. They are reproduced by `measurements/probe.sh`; results are in `measurements/relative_paths.txt` (eBPF) and `measurements/strace_summary.txt` (strace). "Fixture" means `two_binaries_diverge`; "larger build" means `cargo build --release -p waybill-common` with a fresh target dir (31 rustc, 12 cc, 8 build scripts).

## R1 — How many relative opens are there, and is a directory-walk filter still needed?

**Measured.** strace of the fixture and the larger build:

| build | absolute | relative, `AT_FDCWD`, ok | relative, `AT_FDCWD`, failed | relative, directory fd |
|---|---:|---:|---:|---:|
| fixture | 1,163 | 4 (the crate roots) | 2 (`libgcc_s.so.1` loader probes) | 5 (toolchain name ×3, `raw-dylibs` ×2) |
| larger | 10,319 | 28 (crate sources, build-script probes) | 12 (loader probes) | 15 (toolchain name ×3, `raw-dylibs` ×12) |

Relative opens are 0.9% and 0.5% of all opens. The ~12,000-event relative flood that justified milestone 213's rule does not occur. The eBPF trace agrees: 22 relative events out of 1,801 with the System category off.

**Decision.** Remove the relative-path rule from the kernel classifier entirely. **No directory-walk exclusion is kept.** FR-004 permits one, but nothing measured needs it. Relative opens against a directory fd cannot be resolved (R2), so they are recorded as unresolved and counted (FR-003): 5 per fixture build, 15 per larger build.

**Rejected.** Keeping a narrowed exclusion for `dfd ≠ AT_FDCWD`. That is a kernel test for "relative to a directory fd", not "directory walk": `openat(dirfd, "src/lib.rs")` is a genuine file read through the same call. FR-004 allows exclusion only on positive identification, and the volume does not justify the risk.

## R2 — Where does a relative open's directory come from?

**Measured (strace).**
- `fchdir`: **0 calls** in either build.
- `chdir`: every crate compile `chdir`s **in the spawned child, before `execve`**. In the fixture that is pids 154, 155, 174 and 175, each to the workspace root; the larger build has 41 calls, each to a registry crate's directory. Spawns are `clone(CLONE_VFORK)`-style.
- Processes that don't `chdir` (the `rustc -vV` probes) inherit their parent's directory.

**Decision: a working-directory model built from observed events, in userspace.**
- A process's directory is set by a successful `chdir`. An absolute target replaces it; a relative target resolves against the current directory.
- It is inherited from the parent at fork.
- It becomes unknown after `fchdir`, which would need fd-to-path resolution that isn't available.
- **The traced root is seeded by observation too.** The tracer spawns the command with an explicit working directory equal to its own, so the child performs a `chdir` the probes see like any other. No PID mapping between namespaces is needed (see #1143).
- **Attach mode (`--target-pid`).** The target and anything already running have no observed directory. Their relative opens are unresolved until they `chdir`. This is documented, not worked around.

**Rejected.**
- **Reading the directory in the kernel** (`task->fs->pwd`): this needs `task_struct`/`fs_struct` offsets, without CO-RE in this toolchain. That's the kernel-version fragility milestone 210 rejected for `ppid`.
- **`bpf_d_path`**: allowed only in LSM, fentry, fexit and tracing programs. Milestone 211 removed `vfs_open` for this exact reason.
- **Reading `/proc/<pid>/cwd` from userspace**: compiles finish in about 6 ms (measured, `relative_paths.txt`), so the process is often gone. And event pids are init-namespace pids that a containerised tracer can't look up (#1143).

## R3 — Ordering between lineage, directory, rename and open events

**Decision.** Fork, chdir, rename and open records all go into **the one ring buffer that already carries file events** (`FILE_EVENTS`).
- A BPF ring buffer delivers records to the consumer in reservation order across CPUs.
- The fork tracepoint runs in the parent before the child is scheduled, so a child's fork record precedes its own events.
- A `chdir` or `rename` is reported at syscall exit, so it precedes the same thread's next open.

**Rejected.** A separate ring buffer for lineage events, which would require merging two streams by timestamp in userspace.

## R4 — Reporting only successful `chdir` and `rename`

**Decision.** Use the syscall entry and exit tracepoints:
- `sys_enter_chdir` / `sys_exit_chdir`, and `fchdir`;
- `renameat` and `renameat2` (arm64 has no `rename` syscall; x86-64 adds `sys_enter_rename`).

The entry tracepoint stores the path argument in a per-thread pending map. The exit tracepoint emits the record when the return value is 0, and always clears the entry. A tracepoint missing on a kernel is reported the way kprobe attach failures already are. The directory model then degrades to "unresolved" instead of guessing (Principle III).

**Rejected.** A kprobe on `do_renameat2` or `set_fs_pwd`: internal functions whose arguments change shape between kernels, and which fire on attempts rather than successes.

## R5 — Classifying writes

**Found (code).**
- Both open probes write `flags = 0` (`waybill-ebpf/src/programs/file_ops.rs`), and userspace maps every open to Read (`trace/aggregator.rs:295`).
- `vfs_read_entry` and `vfs_write_entry` are not attached (`trace/loader.rs:159-165`), and would emit no path anyway.

**Measured (strace).** Outputs are opened with `O_WRONLY` or `O_RDWR`, plus `O_CREAT|O_TRUNC`. The linker opens the binary itself that way (`deps/safe_only-<hash>`, `O_RDWR|O_CREAT|O_TRUNC`).

**Decision.**
- Record the open flags:
  - `do_filp_open`'s third argument is `const struct open_flags *`, whose first member, `int open_flag`, has been at offset 0 since 3.x (`fs/internal.h`);
  - `do_sys_openat2`'s `struct open_how *` has `u64 flags` at offset 0, and that layout is UAPI.
- Userspace records an open as a write when the access mode is `O_WRONLY` or `O_RDWR`, or `O_CREAT` or `O_TRUNC` is set; anything else is a read.
- Also record `dfd` (both probes' first argument), so userspace can tell cwd-relative opens (`AT_FDCWD`, -100) from directory-fd-relative ones (R1).

**Known and left alone.** Every open hits both probes, so it is recorded twice (measured: each crate root twice). That's existing behaviour and harmless to sets. Removing it is out of scope.

## R6 — Library outputs arrive by rename

**Measured (strace).** `rustc` writes a temporary file, then renames it into place:
- `deps/rmeta*/full.rmeta` → `deps/lib<crate>-<hash>.rmeta`;
- `deps/.tmp*.temp-archive/tmp.a` → `deps/lib<crate>-<hash>.rlib`.

The final library path is never opened for writing. Cargo then hard-links outputs to user-facing names (`linkat deps/safe_only-<hash> → release/safe-only`).

**Decision.**
- Capture successful renames (R4).
- When the renaming process belongs to an invocation whose write set contains the old path, the entry moves to the new path (FR-010).
- The attestation's file operations record the rename's new path as a write.
- Hard links made by cargo are out of scope: they are not compiler writes, and they're #1142's concern.

## R7 — The kernel filter drops build outputs and inputs

**Found (code, `waybill-common/src/filter.rs`).** The CargoFingerprint category's `/deps/` pattern matches everything under the build directory's `deps/`. That covers every rlib, rmeta and object file the compiler writes or reads, and the linked binary.

**Decision.**
- Remove `/deps/` from the CargoFingerprint patterns.
- Keep `/.fingerprint/` and `/incremental/`, the bookkeeping milestone 213 targeted.
- Re-measure the event volume and overflow count with the eBPF harness (SC-003 requires zero overflows). The integration harness's leak check changes accordingly: it currently fails the build if any `/deps/` path appears.

## R8 — Where the change is visible

- **Attestation file operations:**
  - resolved relative opens appear as absolute paths;
  - unresolved ones carry a new flag;
  - writes and renames appear as writes.
- **Compiler pipeline:** read and write sets are populated.
- **Trace integrity:** gains a count of unresolved relative opens.
- **The default witness format** builds its *products* from write operations (`attestation/witness_builder.rs:75`), and products are empty today. With writes captured, products appear. This is intended, and the contract records it.
- **C130/C131: no change.** No production path emits them yet (#1142).

## R9 — Verification without a kernel

The resolution model, flag classification, rename handling and the classifier change are pure functions. They are unit-tested in `waybill-cli` and `waybill-common`. The kernel side is verified by the eBPF integration harness, which CI runs in the eBPF lane (`ci.yml`, `scripts/ebpf-container-run.sh`), so FR-007's assertions become a CI gate.
