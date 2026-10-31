//! #1252 — capture must not depend on an internal kernel function name.
//!
//! FR-012 and SC-004 ask for this to be enforced by a test rather than by
//! review, because review is what let it stand for two releases: the
//! `do_filp_open` kprobe stopped attaching on one runner image, file-open
//! events fell from 902 to 443 on an identical build, and the attestation
//! reported itself complete.
//!
//! These tests read the source tree. That is unusual, and it is the point:
//! the property is "no kprobe is attached anywhere in the trace loader",
//! which is a property of the code rather than of any one run. A runtime
//! test could only observe the kernels it happens to run on.

use std::fs;
use std::path::PathBuf;

fn repo_file(rel: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // waybill-cli -> workspace root
    p.push(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("could not read {}: {e}", p.display()))
}

/// FR-005a and FR-012: the loader cannot attach a kprobe, because the
/// helper that did was deleted.
///
/// Asserted on the helper's absence rather than on a list of permitted
/// attach sites. A list can grow an entry while the test still passes; a
/// missing helper means reintroducing a kprobe requires writing the attach
/// code from scratch, which is visible in review and fails here.
#[test]
fn the_trace_loader_cannot_attach_a_kprobe() {
    let loader = repo_file("waybill-cli/src/trace/loader.rs");

    assert!(
        !loader.contains("fn attach_kprobe"),
        "attach_kprobe is back in trace/loader.rs. A kprobe targets an \
         internal kernel function, whose presence as a traceable symbol is \
         promised to nobody — see #1252 and \
         waybill-ebpf/src/programs/open_syscalls.rs for what to use instead."
    );
    assert!(
        !loader.contains("KProbe"),
        "trace/loader.rs refers to aya's KProbe type again; the loader must \
         attach only tracepoints and uprobes (#1252)."
    );

    // The control. Without it this test would also pass against a loader
    // that attached nothing at all, or a file that failed to be read as
    // something other than an error.
    assert!(
        loader.contains("fn attach_tracepoint"),
        "control: the loader must still attach tracepoints"
    );
    assert!(
        loader.contains("fn attach_uprobe"),
        "control: the loader must still attach uprobes — Principle II \
         names them for TLS interception"
    );
}

/// FR-001 and FR-002: the names of the removed probes' targets must not
/// come back as attach targets.
///
/// The strings may still appear in prose — the #1252 narrative is the
/// evidence for why this feature exists, and deleting it would lose the
/// reason. So this checks the loader, where a name can only be an attach
/// target, rather than the whole tree.
#[test]
fn no_internal_kernel_symbol_is_an_attach_target() {
    let loader = repo_file("waybill-cli/src/trace/loader.rs");
    for symbol in ["do_sys_openat2", "tcp_v4_connect", "vfs_open"] {
        assert!(
            !loader.contains(symbol),
            "trace/loader.rs names the internal kernel function `{symbol}`. \
             Capture must attach to syscall tracepoints, whose names and \
             argument layouts are UAPI (#1252)."
        );
    }
    // `do_filp_open` is checked separately: it appears in this file only
    // inside the comment recording why it was removed, so the assertion is
    // on attach shape rather than on the bare name.
    assert!(
        !loader.contains("\"do_filp_open\""),
        "trace/loader.rs passes `do_filp_open` as an attach target again."
    );
}

/// FR-011: the stability rationale lives next to the capture code.
///
/// This is the requirement that exists to stop the dependency coming back,
/// and its only mechanism is that somebody reaching for an interface finds
/// the reasoning where they are already looking. Documentation elsewhere in
/// `docs/` does not satisfy it, so the test is about placement.
#[test]
fn the_capture_code_records_which_interfaces_are_stable() {
    let module = repo_file("waybill-ebpf/src/programs/open_syscalls.rs");
    let head: String = module.lines().take_while(|l| l.starts_with("//!")).collect();
    assert!(
        head.contains("UAPI"),
        "open_syscalls.rs must state in its own module docs why syscall \
         tracepoints are the stable interface (FR-011)"
    );
    assert!(
        head.contains("kprobe"),
        "open_syscalls.rs must state in its own module docs what must not \
         be attached, not only what should (FR-011)"
    );
    assert!(
        head.len() > 400,
        "the rationale is {} bytes; FR-011 asks for why, not a label",
        head.len()
    );
}
