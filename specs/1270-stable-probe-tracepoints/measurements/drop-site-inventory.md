# Every place a kernel program can lose or fabricate a record

#1185 began with one line: `let _ = PENDING_OPEN.insert(&tid, &pending, 0)`.
A discarded `Result` on a bounded, machine-wide map, at the one site in the
open path that recorded nothing when it returned early. It was found by
reading, not by a check, and reading is not a method — so this enumerates
the rest.

97 candidate sites across the seven kernel programs, by
`grep -nE "return|let _ = |unwrap_or|\.is_err\(\)"` with comment lines
excluded. The grep over-counts — ordinary control flow matches it — and
that is deliberate: a conservative pattern that produces rows to classify
beats a precise one that misses a site, which is the whole subject here.

(An earlier draft of this document said 108. That was the same grep
counting its own comment lines, which is the kind of figure that reads as
a measurement and is not one.)

`scripts/audit-drop-sites.sh` diffs the live sites against the committed
list, so a new one cannot appear unexamined.

## The categories

**SCOPE** — an intentional filter. Not loss: the record was never in
scope. `should_trace()`, the subtree gate, the compiler whitelist.

**COUNTED** — a drop with a named `EventDropReason` slot behind it. The
record is gone and the attestation says so.

**SILENT-DROP** — returns or discards a `Result` without recording
anything. This is #1185's class.

**SILENT-SUBSTITUTION** — an unreadable value is replaced by a default and
the record is emitted anyway. **Worse than a drop**, because a drop can be
counted whereas this produces a plausible, wrong record that nothing
distinguishes from a right one. This category did not exist when the
inventory started; it emerged from the enumeration.

## SILENT-SUBSTITUTION

The ones that change what a record *means*, ordered by consequence.

| site | substitutes | consequence |
|---|---|---|
| `open_syscalls.rs:457` | openat2 `flags` ← `0` when the `how` struct is unreadable | `0` is `O_RDONLY`, so **a write is recorded as a read**. `is_write()` keys off these flags, so the operation lands in a read set and the write set loses it |
| `open_syscalls.rs:88`, `fs_syscalls.rs:43` | `ret` ← `-1` when the exit's return value is unreadable | `-1` means "the syscall failed", so a **successful** open is discarded as a failure and counted in `open_failures_discarded` — a slot that asserts the syscall failed when what actually happened is that we could not tell |
| `open_syscalls.rs:426`, `:460`, `fs_syscalls.rs:181`, `:183` | `dfd` ← `AT_FDCWD` | a directory-relative open is resolved as if it were relative to the working directory, so m1070 resolves it against the wrong base and reports it `Resolved` rather than `Unresolved` |
| `compiler_exec.rs:166`, `:447` | `comm` ← `[0u8; 16]` | an empty comm matches no whitelist entry, so a compiler exec is silently not tracked and its invocation never exists |
| `compiler_exec.rs:226`, `:231` | ppid ← `0` | the ancestry walk terminates early, `ppid` is emitted as 0, and the DAG edge for that invocation is missing — the #610 symptom |
| `open_syscalls.rs:427`, `:428`, `:453`, `:461`, `:495`, `:496`, `:515`, `fs_syscalls.rs:125`, `:182`, `:184`, `:189`, `:191`, `:227`, `:230` | path pointers ← `0`, flags ← `0` | a null path reaches `KERNEL_NULL_PATH`, which **is** counted; the flags cases are the `:457` problem in milder form |

The path-pointer substitutions are the benign end: they converge on a
counted slot. The first five rows do not.

## SILENT-DROP

Discarded `Result`s on bounded maps — #1185's own class, in four more
places:

| site | map | consequence when it fails |
|---|---|---|
| `compiler_exec.rs:247` | `COMPILER_INVOCATIONS` (4096) | the compiler is not tracked; every file op it performs belongs to no invocation |
| `compiler_exec.rs:252` | `COMPILER_DIRECT_EXECS` (4096) | the ancestry walk terminates at this pid, so descendants resolve the wrong parent |
| `compiler_exec.rs:303` | `PID_TO_PPID` (32768) | `ppid` resolves to 0 for this process and its DAG edge is lost |
| `compiler_exec.rs:310` | `COMPILER_INVOCATIONS` | a forked child does not inherit its parent's invocation |
| `fs_syscalls.rs:53` | `PENDING_CHDIR` | the `chdir` is never joined, so the process's working directory stays stale and **every later relative read by it and its descendants resolves against the wrong base** |
| `fs_syscalls.rs:196` | `PENDING_RENAME` | the rename is never joined and the write set loses the destination |

`fs_syscalls.rs:53` is the one to take seriously. It is the same shape as
the open-path bug but amplifying: a lost `chdir` does not lose one record,
it corrupts the base path for a whole subtree of the build. m1070 already
documents cwd inheritance as the amplifier; nothing counts a failure here.

Network paths, uncounted throughout:

| site | consequence |
|---|---|
| `tcp_connect.rs:60`, `:87`, `:91`, `:94` | a connect is dropped — unparseable address family, unreadable args |
| `tcp_connect.rs:116` | `CONN_INFO` (4096) full: the connection is recorded with no metadata to join later |
| `tls_openssl.rs:25`, `:56`, `:61`, `:150` | a TLS read or write is dropped |
| `tls_openssl.rs:104`, `:183` | `bpf_probe_read_user_buf` fails and the result is discarded, so a plaintext record is **emitted with an unfilled buffer** — a SILENT-SUBSTITUTION in the network path |

## COUNTED

`open_syscalls.rs` 114, 184, 204, 244, 283, 299; `fs_syscalls.rs` 101, 105;
`file_ops.rs` 72, 126; `compiler_exec.rs` 335 — `KERNEL_PATH_UNREADABLE`,
`KERNEL_NULL_PATH`, `KERNEL_EXIT_WITHOUT_REQUEST`,
`KERNEL_PENDING_INSERT_FAILED`, `KERNEL_OUTSIDE_TRACED_SUBTREE`,
`KERNEL_TRACED_INSERT_FAILED`, and the m212 ring-buffer counters.

## SCOPE

`open_syscalls.rs` 100, 156–160, 208; `compiler_exec.rs` 171, 356, 427,
432; `fs_syscalls.rs` 63, 67. The handshake, `should_trace()`, the subtree
gate, the compiler whitelist, and the pending-entry joins. Returning here
is the feature.

## What the inventory answers, and what it does not

It answers the question #1185 could not: **how many more silent losses are
there?** Ten SILENT-DROP sites and six consequential SILENT-SUBSTITUTION
sites, none counted. Four of the ten are discarded map inserts of exactly
the kind that started this.

It does **not** establish that any of them fires. Every row is a reading of
the source, not a measurement — the same standard of evidence that made
`COMPILER_INVOCATIONS`'s doc comment wrong for years. Each needs the
treatment `kernel_pending_insert_failed` got: a counter, then a forced
control proving the counter can fire, then a measurement.

Ranked by consequence if it does fire, highest first:

1. `fs_syscalls.rs:53` — a lost `chdir` corrupts a whole subtree's paths.
2. `open_syscalls.rs:457` — a write recorded as a read.
3. `open_syscalls.rs:88` / `fs_syscalls.rs:43` — a successful open
   discarded as failed, under a counter that claims it failed.
4. `compiler_exec.rs:247` / `:310` — a compiler or its child untracked.
5. The `dfd ← AT_FDCWD` substitutions — a path resolved against the wrong
   base and reported as resolved.
