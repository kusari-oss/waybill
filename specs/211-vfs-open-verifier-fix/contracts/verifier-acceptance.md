# Contract: eBPF verifier acceptance

**Milestone**: 211
**Date**: 2026-07-20
**Purpose**: Lock the observable properties of the post-m211 eBPF programs against which reviewers audit any m211 or follow-up change.

## C-1: `vfs_open_entry` bytecode loads on Colima aarch64 Linux 6.8

**Observable**: `docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug mikebom-ebpf-test 2>&1 | grep -c 'could not attach vfs_open kprobe'` returns `0`.

**Emission gate**: mikebom loader emits NO `WARN mikebom::trace::loader::inner: could not attach vfs_open kprobe` line at trace startup.

**Failure mode if regressed**: WARN reappears + `.predicate.file_access.operations` stays empty in the emitted attestation.

## C-2: `do_filp_open_entry` bytecode loads on Colima aarch64 Linux 6.8

**Observable**: Same container-harness invocation → `grep -c 'could not attach do_filp_open kprobe'` returns `0`.

**Emission gate**: mikebom loader emits NO `WARN` about `do_filp_open` attachment failure.

**Rationale**: Per Clarification Q2 + US-1b, both kprobes are in scope for this milestone. Regression on `do_filp_open` post-m211 is a spec-scope failure.

## C-3: `file_access.operations[]` populates on real cargo builds

**Observable**: The emitted attestation from the SC-001 fixture (m210's `two_binaries_diverge`) has `.predicate.file_access.operations | length > 100`.

**Rationale**: A `cargo build --release` for 4 tiny crates opens hundreds of files (sources, `.rlib`s, target/incremental, /etc/, /proc/, /usr/include/, etc.). Floor set at 100 to accommodate FR-016-style filter-adjacent variance; typical uncapped count on this fixture is 5000+.

**Failure mode if regressed**: `file_access.operations` is empty or trivially small (< 100). Symptom cascades: m210's `mikebom:source-read-set` (C130) can't populate → downstream reachability tooling gets `"unknown"` for every component.

## C-4: `FileEvent` wire shape is byte-identical to pre-m211

**Observable**: Compare a golden attestation from a scan-mode trace (which doesn't invoke either fixed kprobe) pre-m211 vs. post-m211. Every JSON field name, type, and (for non-file-op fields) value MUST be byte-identical.

**Regression guard**: extend the milestone-210 container-test harness assertion:
```
diff <(pre-m211-attestation.json | jq 'del(.predicate.file_access, .predicate.trace_integrity.kprobe_attach_failures)') \
     <(post-m211-attestation.json | jq 'del(.predicate.file_access, .predicate.trace_integrity.kprobe_attach_failures)')
```
MUST return empty.

**Failure mode if regressed**: field name/type drift = wire-shape violation of FR-003. Any userspace consumer parsing the attestation JSON breaks silently.

## C-5: On kernels where the fix still rejects, log volume stays under 5 KB

**Observable**: `stderr` output from `mikebom trace run` on a kernel that rejects the post-fix bytecode contains total mikebom logging under 5 KB (the verifier-dump inline WARN is truncated to under 500 bytes per FR-008).

**Regression guard**: unit test in `mikebom-cli/src/trace/loader.rs::tests` that simulates a verifier-rejection error (constructing an `aya::EbpfError::Program` with a 20 KB message body) and asserts the resulting WARN line is under 500 bytes.

**Failure mode if regressed**: Log-noise regression; operators on unsupported kernels get flooded with unhelpful verifier-dump text.

## C-6: `RUST_LOG=aya=debug` escape hatch preserves full verifier trace

**Observable**: With `RUST_LOG=aya=debug` set, the operator MUST be able to retrieve the full verifier dump from the aya crate's own log output.

**Rationale**: The FR-008 500-byte cap on mikebom's OWN log line MUST NOT hide the full trace when a developer specifically asks for it. aya's `debug!` logs preserve the full dump; mikebom's WARN just trims for the default case.

**Failure mode if regressed**: Debugging a new kernel-version verifier rejection becomes impossible without patching mikebom.

## C-7: `trace_integrity.kprobe_attach_failures[]` accurately enumerates failures

**Observable**: `.predicate.trace_integrity.kprobe_attach_failures` in the emitted attestation MUST contain each kprobe name that failed to attach at trace start, with no duplicates.

**Regression guard**: on a kernel where BOTH `vfs_open` AND `do_filp_open` fail (a hypothetical strict-verifier kernel), the array contains exactly `["do_filp_open", "vfs_open"]` (sorted lex) — not one entry, not none.

**Failure mode if regressed**: Operators lose the transparency signal (Constitution Principle X); silent-empty file_access on unsupported kernels can't be distinguished from silent-empty on a hermetic build.
