//! Milestone 210 T047 — stdin-input FR-018 integration test.
//!
//! Verifies end-to-end that when a compiler invocation reads from stdin
//! (via `gcc -x c -` or `/dev/stdin`), the emitted SBOM's per-component
//! `waybill:source-read-set` (C130) carries the synthetic `<stdin>`
//! entry with `kind.stdin_input.bytes_read` — NOT a masqueraded regular
//! `file` entry with the literal `-` path.
//!
//! Gated behind `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`
//! per the m210 integration-test convention: the eBPF trace path only
//! functions on Linux with `CAP_BPF`; on other hosts the test compiles
//! as an empty module (verified by the pre-PR gate on macOS).
//!
//! The T017 fixture at `tests/fixtures/compiler_pipeline/stdin_input/`
//! is deferred to the Linux session per the tasks.md deferral note —
//! this file is scaffolded now so the code-side detection (unit-tested
//! in `waybill-cli/src/trace/compiler_pipeline.rs`) has a documented
//! end-to-end verification path once the fixture materializes.

#![cfg(all(target_os = "linux", feature = "ebpf-tracing"))]

// The integration test body is authored during the Linux session when
// (a) the T017 fixture is vendored (`gcc -x c - < input.c`, expected
// output SBOM committed as golden) and (b) a `CAP_BPF`-capable runner
// is available in CI to actually attach the eBPF probes. Until then
// the module compiles as-is on Linux+ebpf-tracing hosts and is a
// no-op elsewhere.
//
// Expected structure once the fixture lands:
//
// ```
// #[test]
// fn stdin_input_lands_as_synthetic_stdin_entry_not_dash_path() {
//     // 1. spawn `waybill trace run -- gcc -x c - < <(cat main.c)`
//     //    with attestation output at a tempdir path
//     // 2. parse the emitted attestation JSON
//     // 3. locate the produced binary component in the SBOM
//     // 4. assert its `waybill:source-read-set` payload's read_set
//     //    array contains ONE entry with path == "<stdin>" and
//     //    kind.stdin_input.bytes_read >= 1 (matching input.c length)
//     // 5. assert NO entry with path == "-" or "/dev/stdin" leaks
//     //    through as a `file`-kind entry
// }
// ```
