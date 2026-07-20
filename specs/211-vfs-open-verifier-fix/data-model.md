# Data Model: vfs_open verifier fix

**Milestone**: 211
**Date**: 2026-07-20
**Status**: Phase 1

## Overview

This milestone does NOT introduce new persistent types. Every entity involved is a **frozen or in-process** structure already defined in mikebom's existing crates. The primary change is the internal init pattern of an existing kernel-side program — no wire-visible types shift.

## E1 — `FileEvent` (frozen per FR-003)

**Ownership**: `mikebom-common/src/events.rs`.
**Lifecycle**: constructed kernel-side in each file-op kprobe program → written into `FILE_EVENTS` ring buffer → deserialized userspace-side → aggregated into `FileAccess.operations[]` on the emitted attestation.

**Fields** (per current `mikebom-common/src/events.rs`, unchanged in this milestone):

| Field | Type | Bytes | Semantics |
|---|---|---|---|
| `event_type` | `FileEventType` (u8 enum) | 1 | `Open` \| `Read` \| `Write` \| `Close` |
| _pad_ | u8×7 | 7 | struct alignment |
| `timestamp_ns` | u64 | 8 | `bpf_ktime_get_ns()` at kprobe fire time |
| `pid` | u32 | 4 | process PID |
| `tid` | u32 | 4 | thread ID |
| `comm` | [u8; 16] | 16 | 16-byte kernel-limited `comm` field |
| `path` | [u8; 256] | 256 | canonical pathname, NUL-terminated when < 256 bytes |
| `path_truncated` | u8 | 1 | `1` iff `bpf_d_path` returned `n == 256` (potentially truncated) |
| `_path_padding` | u8×3 | 3 | struct alignment |
| `flags` | u32 | 4 | open flags |
| `bytes_transferred` | u64 | 8 | for Read/Write events |
| `content_hash` | [u8; 32] | 32 | SHA-256 of contents, populated userspace-side post-close |
| `inode` | u64 | 8 | file inode number when available |

Total: ~352 bytes. **Frozen — no changes in m211.**

**Validation rules** (unchanged):
- `path[0..path_truncated_offset]` MUST be valid UTF-8 or userspace uses `String::from_utf8_lossy`.
- `path_truncated` interpretation refined in m211: `1` post-fix means "path filled the 256-byte buffer, potentially truncated" (see R8 in `research.md`). Consumers who need certainty in the sub-256-byte case still get it (path is definitely complete if `path_truncated == 0`).
- `content_hash: [u8; 32]` MAY be all-zero if the file was closed without hash computation (kernel-side left blank; userspace computes on stat).

**Cross-milestone reference**: milestone 210's `CompilerExecEvent` uses the same `#[repr(C)]` + ringbuf-reserve pattern. Same verifier-hardening lessons apply to both structs.

## E2 — Kernel-side scratch structures

None introduced. The fix removes explicit zero-init lines rather than introducing new structures. If R2 Alt A ("use `core::ptr::write_bytes`") becomes necessary as a fallback, it uses raw pointer arithmetic on the existing `FileEvent`, not a new struct.

## E3 — `FILE_EVENTS` ring buffer (unchanged)

**Ownership**: `mikebom-ebpf/src/maps.rs:19`.
**Capacity**: `RingBuf::with_byte_size(128 * 1024 * 1024, 0)` — 128 MB, sized for high-throughput builds.
**Lifecycle**: kernel produces → userspace drains at 5 ms cadence.

Post-fix, this map fills with real events from `vfs_open` + `do_filp_open` (previously empty from vfs_open since the program failed to load). Existing overflow accounting via `trace_integrity.ring_buffer_overflows` remains unchanged; if the fix suddenly floods the ring buffer, existing back-pressure signals fire.

**Sizing decision**: no change. 128 MB was already sized for the "with vfs_open loaded" case — the pre-fix empty state was an accidental under-utilization.

## E4 — `TraceIntegrity.kprobe_attach_failures[]` (populated by the loader)

**Ownership**: `mikebom-cli/src/trace/loader.rs` populates this on `attach()` failure; `mikebom-common` defines the field on `TraceIntegrity`.
**Semantics**: array of kprobe names (as `String`s, e.g., `"vfs_open"`, `"do_filp_open"`) that failed to attach at trace startup.

**Post-fix behavior**:
- On kernels within the FR-001 support matrix (Colima 6.8 aarch64 + Ubuntu 22.04/24.04 amd64 6.5+): `vfs_open` MUST NOT appear here. `do_filp_open` MUST NOT appear here.
- On best-effort older kernels (5.15 LTS, 5.10, etc.): one or both MAY appear — per Constitution Principle X (Transparency), operators see the degradation instead of getting a silent empty `file_access.operations[]`.

## E5 — `attach_kprobe` WARN message (rate-limited per FR-008)

**Ownership**: `mikebom-cli/src/trace/loader.rs::attach_kprobe`.
**Current behavior**: unbounded — logs `format!("could not attach {kprobe} kprobe: {e}")` where `{e}` may be ~20 KB of verifier trace.
**Post-fix behavior**: capped at ≤500 bytes total. Format:
```
could not attach {kprobe} kprobe on this kernel: {short_reason}. \
file_access will be empty. See docs/architecture/attestations.md#file-op-tracing-gaps.
```
Where `{short_reason}` is the LAST line of the aya error output (typically `"processed NNN insns (limit 1000000)"` or `"BPF program is too large"`).

Rate-limit target: one WARN per (kprobe name, PID) pair per trace invocation. Duplicate suppression not required in MVP since kprobe attach only happens once at trace startup.

## State transitions

No new state machines. The existing "kprobe attach → success | failure" transition simply moves more programs to the "success" branch on the FR-001 target kernels.

## Persistence

None. All state remains in-process for the duration of a single trace, mirroring milestones 001–210.
