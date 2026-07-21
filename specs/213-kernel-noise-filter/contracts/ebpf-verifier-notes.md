# Contract: eBPF verifier acceptance for `path_matches_filter_category`

**Feature**: 213-kernel-noise-filter
**Kind**: Verifier-acceptance contract (kernel-side)
**Consumers**: any kernel-side change to `path_matches_filter_category` in `mikebom-ebpf/src/programs/file_ops.rs` — MUST re-verify acceptance before merge.

## Kernels in the acceptance matrix

| Kernel version | Distro/env               | Verified how                          |
|-----|--------------------------|---------------------------------------|
| 5.15 LTS         | Ubuntu 22.04 (CI)        | Container harness (m212 pattern)      |
| 6.1 LTS          | Debian 12 (CI)           | Container harness                     |
| 6.6              | Ubuntu 24.04 (CI)        | Container harness                     |
| 6.8              | Colima aarch64 (dev)     | Container harness                     |

Rejection on ANY kernel is a merge-blocker per FR-013 + SC-004.

## The verifier-safe recipe (pinned by m211)

The kernel-side classifier MUST follow these patterns (established in m211
issue #611, proven by kernel loader accept on 5.15/6.1/6.6/6.8):

### Rule 1: Fixed-size byte arrays, not slice-of-slices

```rust
// ✅ CORRECT — fixed-size, no fat pointers
#[repr(C)]
struct PathPattern {
    bytes: [u8; 32],
    len: u8,
    category: u8,
    _pad: [u8; 6],
}
const PATTERNS: [PathPattern; 15] = [...];

// ❌ WRONG — slice-of-slices confuses the verifier (m211 failure class 1)
const PATTERNS: &[&[u8]] = &[b"/etc/", b"/proc/", ...];
```

### Rule 2: Word-wide u64 compares, not byte loops

```rust
// ✅ CORRECT — 4 iterations of u64 compare (loop unrollable, bounded)
for i in 0..4 {
    let pattern_word: u64 = read_u64_le(&pattern.bytes, i * 8);
    let path_word: u64 = read_u64_le(&path, i * 8);
    if pattern_word != path_word { return false; }
}

// ❌ WRONG — variable-length byte loop (m211 failure class 1 + 5)
for i in 0..pattern.len {
    if pattern.bytes[i] != path[i] { return false; }
}
```

### Rule 3: `#[inline(always)]` for called helpers

```rust
// ✅ CORRECT — inlined; the verifier sees a single monolithic function
#[inline(always)]
fn path_matches_prefix(pattern: &PathPattern, path: &[u8; 256]) -> bool { ... }

// ❌ WRONG — non-inlined helper crosses a function-call boundary the
// verifier state-tracks; the effective instruction cost multiplies.
fn path_matches_prefix(pattern: &PathPattern, path: &[u8; 256]) -> bool { ... }
```

### Rule 4: No BPF helper calls per-category

Do NOT call `bpf_probe_read_kernel`, `bpf_probe_read_user_str_bytes`, or any
other BPF helper from inside the classifier. The path bytes are ALREADY in
the stack-local `[u8; 256]` buffer (populated once at kprobe entry). The
classifier operates on those bytes.

### Rule 5: No BPF-restricted helpers

Per m211's `bpf_d_path` retirement — do NOT call any helper that the
verifier restricts to LSM/fentry/fexit program types (`bpf_d_path`,
`bpf_get_current_pid_tgid` in specific contexts, etc.). The classifier's
input is a plain byte array, not a kernel struct pointer, so this is
naturally satisfied by design.

## Verifier cost estimate

Per open, assuming path bytes are already in `[u8; 256]`:

| Component                                | Instructions |
|------------------------------------------|--------------|
| Read `FILTER_WIDEN[0]` (widen flag)      | ~15          |
| Category loop (15 patterns × 12 insns avg) | ~180       |
| Match-found branch (increment hit + return) | ~10       |
| Match-not-found early-return             | ~5           |
| **Per-open budget**                      | ~210 insns   |

Total classifier + existing kprobe code: ~800 insns (well under 1M budget).

## Load-time verification recipe

After any change to `path_matches_filter_category`:

```bash
# On Colima (dev):
docker build -f Dockerfile.ebpf-test -t mikebom-ebpf-test .
docker run --rm --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    mikebom-ebpf-test \
    bash -c "/mikebom/target/release/mikebom trace capture --output /tmp/x -- true && bpftool prog show"

# Expected output: all mikebom-* programs listed, no "verifier rejected"
```

If ANY kernel in the SC-003 matrix rejects the program, the change MUST be
reverted or the recipe (Rules 1–5) MUST be re-applied to fix the offending
construct. There is no "just skip that kernel" escape hatch — SC-004 pins
verifier acceptance on all four kernels as a merge condition.
