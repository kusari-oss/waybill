//! A panicking test's backtrace names its source file and line (spec 1661).
//!
//! This guards the debug-info configuration of test builds. `.cargo/config.toml`
//! changes how macOS packages debug info, and CI could be set to build with
//! less of it to compile faster. Either is fine only while a failing test still
//! points at its source. With no debug info this test fails, and that is the point.
//!
//! The test re-runs its own executable as a child that panics with
//! `RUST_BACKTRACE=1`, then checks a backtrace frame for `at <this file>:<line>`.

#![allow(clippy::unwrap_used)]

use std::process::Command;

const CHILD: &str = "WAYBILL_BACKTRACE_PROBE_CHILD";

#[test]
fn a_panic_backtrace_names_this_file_and_line() {
    if std::env::var(CHILD).as_deref() == Ok("1") {
        panic!("backtrace probe");
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "a_panic_backtrace_names_this_file_and_line", "--nocapture"])
        .env(CHILD, "1")
        .env("RUST_BACKTRACE", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "the child must panic; stderr:\n{stderr}");
    assert!(stderr.contains("backtrace probe"), "the child panicked for another reason; stderr:\n{stderr}");
    // A frame's `at <file>:<line>` line, not the panic message: the message's
    // `panicked at <file>:<line>` comes from the panic location, which is
    // present without debug info, so it can't tell the two apart. Measured,
    // spec 1661 measurements/local.md (T006).
    let named = regex::Regex::new(r"(?m)^\s+at \S*backtrace_resolves_source\.rs:\d+").unwrap();
    assert!(
        named.is_match(&stderr),
        "the backtrace does not name this file and a line, so debug info is missing \
         or unreadable in this build; stderr:\n{stderr}"
    );
}
