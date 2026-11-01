# Data Model: Capture that survives a kernel change

Feature: `specs/1270-stable-probe-tracepoints/` (spec-kit sequence 1270,
which is **not** an issue number) · Issue **#1252**

Three things change shape: one new kernel-side map entry, one new
record kind on an existing event, and one new field on
`TraceIntegrity`. Nothing else in the attestation moves — the read and
write sets, `FileEvent`, `NetworkEvent` and `IpAddr` all keep their
current layout.

---

## 1. `PendingOpen` — an open between its request and its result

New, in `waybill-ebpf/src/maps.rs` next to `PendingRename`
(`maps.rs:205`), which it deliberately mirrors.

```rust
/// Arguments of an in-flight open, by thread id. See `PENDING_CHDIR`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PendingOpen {
    pub dfd: i64,
    pub path_ptr: u64,
    pub flags: u32,
    pub _padding: u32,
}

#[map]
pub static PENDING_OPEN: HashMap<u32, PendingOpen> =
    HashMap::with_max_entries(4096, 0);
```

| Field | Source at `sys_enter` | Note |
|---|---|---|
| `dfd` | `ARG0` (offset 16) | `AT_FDCWD` for `open`/`creat`, which take no dfd |
| `path_ptr` | `ARG1` for `openat`/`openat2`, `ARG0` for `open`/`creat` | A user pointer, read as a string at exit |
| `flags` | `ARG2` for `open`/`openat`; `open_how.flags` via a **user** read for `openat2`; `O_WRONLY\|O_CREAT\|O_TRUNC` for `creat` | See research R4 — a kernel read here compiles and silently yields 0 |

`_padding` keeps the struct a multiple of 8 bytes, as `PendingRename`
does implicitly.

**Key**: thread id, from `current_tid()`. The same choice m1070 made,
for the same reason: a syscall's entry and exit run on one thread.

**Capacity**: 4096, matching `PENDING_CHDIR` and `PENDING_RENAME`. The
map holds one entry per thread currently inside an open syscall, not
per open, so the bound is concurrent threads rather than event volume.

### State transitions

```
        sys_enter_open*             sys_exit_open*
(absent) ───────────────▶ (pending) ───────────────▶ (absent)
                              │                          │
                              │                     ret >= 0 → submit
                              │                     ret <  0 → discard
                              │
                       thread dies inside
                       the syscall, or the
                       exit is never seen
                              │
                              ▼
                   (pending, stale — overwritten
                    by this thread's next open)
```

Two invariants carry FR-005c, and both are already how `fs_syscalls.rs`
behaves:

1. **The exit handler removes the entry before it inspects the
   result** (`fs_syscalls.rs:65`). A failed open leaves nothing behind.
2. **`insert` overwrites.** A stale entry from a thread that died inside
   a syscall is replaced by that thread's next open rather than being
   read as that open's request. Tid reuse is bounded by the same
   mechanism. This is the "held request whose result never arrives"
   edge case, and it needs no reaper.

---

## 2. `FileEventType::OpenFailed` — the discard record

```rust
    /// An open request the kernel discarded because it did not succeed.
    /// Emitted ONLY when the diagnostic slot is set; userspace logs it
    /// and never admits it to a read or write set. `flags` carries the
    /// negated errno the syscall returned.
    OpenFailed = 8,
```

Appended as variant 8, with the matching `from_raw` arm
(`events.rs:66-76`). Additive: a `FileEvent` carries it, so no new
record size.

| Reused field | Meaning for `OpenFailed` |
|---|---|
| `path` | the path as the process passed it, read at exit |
| `dfd` | the directory the path is relative to |
| `flags` | the negated errno (`-ENOENT` etc.), **not** open flags |

Overloading `flags` is the one shape compromise here. It avoids
widening `FileEvent` for a diagnostic-only record, and the field's
meaning is unambiguous because it is keyed on the event type. The
contract file states it so a consumer cannot read it as open flags.

**A consumer never sees this.** It is a diagnostic event: the
aggregator logs it and returns, so it reaches no read set, no write
set, and no attestation field.

---

## 3. Counters and the diagnostic slot

Two new kernel-side map entries, each following an established pattern
exactly.

```rust
/// Failed opens the kernel discarded. Always counted, even with the
/// diagnostic slot clear: a zero here on a build known to attempt a
/// failed open means the exit tracepoint is not firing, which looks
/// identical to a clean run if nothing counts it.
#[map]
pub static OPEN_FAILURES_DISCARDED: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(1, 0);

/// Set by userspace to 1 to make discarded opens reportable as
/// `OpenFailed` records. Clear by default; the m213 `FILTER_WIDEN`
/// pattern (`maps.rs:191`).
#[map]
pub static OPEN_DIAG: PerCpuArray<u8> =
    PerCpuArray::with_max_entries(1, 0);
```

Read back with `trace/counters.rs:116`'s `read_percpu_sum`, as m212's
counters are.

---

## 4. `TraceIntegrity.open_failures_discarded`

```rust
    /// #1252: open requests the kernel discarded because the syscall
    /// failed. Those requests used to reach the read set, so this is
    /// the size of the overstatement this feature removes. Absent when
    /// zero, so a trace with no failed opens serializes as before.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub open_failures_discarded: u64,
```

Placed **last** in the struct, with `#[serde(default)]`, following the
m1070 `unresolved_relative_opens` and m1072 precedents in
`waybill-common/src/attestation/integrity.rs:36-47`. A pre-#1252
attestation round-trips unchanged, and a trace with zero failed opens
serializes byte-identically to today.

**`kprobe_attach_failures` keeps its name and type.** Per clarification
Q1 it is already a union — it has carried counter-map names since m212
and tracepoint names since m210 — so the only change is its doc
comment, which must stop saying "kprobes" (FR-011a).

---

## 5. What does not change

| Thing | Why it is listed |
|---|---|
| `FileEvent` layout | The join reuses it; only the event type is new |
| `NetworkEvent`, `IpAddr` | `IpAddr` already carries 16 octets and `is_v6` (`ip.rs:8-14`), so IPv6 needs no wire change |
| `CONN_INFO` and `conn_id` | The TLS uprobes correlate through it (`tcp_connect.rs:48`); the replacement must populate it identically or payload correlation breaks silently |
| read and write sets | Their *contents* shrink by failed requests; their shape does not move |
| `PartialReason::ProbeAttachFailed` | Landed in #1310; this feature reuses it |
