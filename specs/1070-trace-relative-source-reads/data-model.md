# Data Model: Trace captures relative source reads and compiler writes

## Kernel → userspace records (`waybill-common/src/events.rs`)

All four kinds travel in the `FILE_EVENTS` ring buffer (research R3).

**Today:**
- every record is a `FileEvent` (352 bytes, pinned by a size test);
- the drain casts every record to it;
- `event_type` is the first field, a `#[repr(C)]` enum (4 bytes).

**After:** userspace reads `event_type` first, then decodes the record type it names and checks that record type's size.

### `FileEvent` (changed)

| field | change |
|---|---|
| `event_type` | unchanged. Opens remain `Open`, and read/write is decided in userspace from `flags`. |
| `path_truncated` | **now populated**: `1` when the path copy filled the 256-byte buffer. A truncated relative path is unresolved. |
| `flags` | **now populated**: the open flags (`open_flag` from `do_filp_open`, `how->flags` truncated to 32 bits from `do_sys_openat2`). The low 32 bits carry `O_ACCMODE`, `O_CREAT` and `O_TRUNC`. |
| `dfd` (new, `i32`) | the directory fd the path is relative to; `-100` (`AT_FDCWD`) means the working directory. The record's size changes, and its size test is updated to the new value. Kernel and userspace come from one crate, so they always agree. |

### `FileEventType` (new variants)

| variant | record | emitted by |
|---|---|---|
| `Fork` | `LineageEvent { parent_pid, child_pid, timestamp_ns }` | `sched_process_fork`, which already exists. It now also emits a record. **`parent_pid` is the parent's process id (tgid) from `bpf_get_current_pid_tgid() >> 32`**, not the tracepoint's `parent_pid` argument, which is the parent *thread's* id. The tracepoint runs in the parent's context. Cargo spawns compiles from worker threads, and keying by thread id would make inheritance miss. `child_pid` is the tracepoint's argument: for a new process its tid equals its tgid; for a new thread the entry is never consulted, because events carry the tgid. |
| `Chdir` | `FileEvent` with `path` = the target as passed, `dfd` = `AT_FDCWD` | `sys_exit_chdir` when the return value is 0 (path from `sys_enter_chdir`) |
| `Fchdir` | `FileEvent` with an empty `path` | `sys_exit_fchdir` when the return value is 0 |
| `Rename` | `RenameEvent { pid, tid, comm, timestamp_ns, old_dfd, old_path[256], new_dfd, new_path[256], truncated flags }` | `sys_exit_renameat` / `renameat2` (and `rename` where it exists) when the return value is 0 |

The noise classifier (R7) applies to `Open` and `Rename`, with Rename tested on the new path. It doesn't apply to `Fork`, `Chdir` or `Fchdir`, which are needed to resolve paths whatever their value.

## Userspace state

### `CwdTracker` (new, `waybill-cli/src/trace/`)

`pid → WorkingDir`, where `WorkingDir` is `Known(PathBuf)` or `Unknown`.

| input | effect |
|---|---|
| `Fork { parent, child }` | `child` ← the parent's entry (`Unknown` if the parent has none) |
| `Chdir { pid, path }` | absolute `path`: `Known(path)`. Relative: `Known(cwd.join(path))` if the current entry is `Known`, otherwise `Unknown`. A truncated path gives `Unknown`. |
| `Fchdir { pid }` | `Unknown` |
| any pid not seen | `Unknown`, i.e. alive before the trace with no observed `chdir` (attach mode, host processes) |

`resolve(pid, dfd, path) -> Resolved(PathBuf) | Unresolved`:
- absolute `path`: `Resolved(path)`;
- `dfd == AT_FDCWD` and the pid's entry is `Known(d)`: `Resolved(d.join(path))`. Lexical join only: no filesystem access and no `..` canonicalisation (spec edge case);
- otherwise (`dfd` is a real fd, or the directory is unknown): `Unresolved`.

### Write classification (pure function)

`is_write(flags) = (flags & O_ACCMODE) ∈ {O_WRONLY, O_RDWR} || flags & (O_CREAT | O_TRUNC) != 0`.

### Rename handling

For `Rename { pid, old, new }`, with both paths resolved through `CwdTracker`:
- **compiler pipeline:** when the pid belongs to an invocation, `new` is inserted into its write set, carrying over `old`'s value if `old` was present, and `old` is removed (FR-010). This happens even if `old` was never observed, for example because its open was noise-filtered;
- **file operations:** a `Write` of `new` is recorded;
- **an unresolved old or new path:** the operation is recorded flagged unresolved and counted, and no write-set change is made.

## Attestation shape (`waybill-common/src/attestation/`)

| type | field | change |
|---|---|---|
| `FileOperation` | `path` | resolved absolute path. For an unresolved open, the path exactly as the build passed it. |
| `FileOperation` | `unresolved_relative` (new, `bool`) | `true` when the path could not be resolved. Serialized only when `true`. |
| `FileOperation` | `operation` | now `Write` for opens-for-write and renames (was always `Read` for opens) |
| `CompilerInvocation` | `read_set` / `write_set` | resolved paths only. Unresolved opens are never inserted (FR-003). |
| `TraceIntegrity` | `unresolved_relative_opens` (new, `u64`) | count of unresolved relative opens and renames. Serialized only when non-zero. |

The new fields are additive and omitted at their default, so a trace with nothing new to report serializes as before (SC-005).

**Witness format.** Operations with `unresolved_relative: true` are excluded from witness materials and products, exactly as they are from read and write sets (FR-003). They remain in `file_access.operations`.
