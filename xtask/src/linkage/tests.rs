// Issue #824 — the gate's own teeth.
//
// A linkage check that passes because it inspected nothing is worse
// than no check at all, so the tests here assert on both directions:
// a baseline dependency is accepted, and an injected third-party one
// is rejected.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use super::baseline::{baseline_for, known_targets};

#[test]
fn platform_baselines_accept_os_supplied_libraries() {
    let linux = baseline_for("x86_64-unknown-linux-gnu").unwrap();
    for dep in ["libc.so.6", "libm.so.6", "ld-linux-x86-64.so.2"] {
        assert!(linux.allows(dep), "linux baseline must accept {dep}");
    }

    // The three the shipping macOS binary actually links today. If this
    // ever fails, the constitution and the product have diverged.
    let macos = baseline_for("aarch64-apple-darwin").unwrap();
    for dep in [
        "/usr/lib/libSystem.B.dylib",
        "/usr/lib/libiconv.2.dylib",
        "/System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation",
        "/System/Library/Frameworks/CoreServices.framework/Versions/A/CoreServices",
    ] {
        assert!(macos.allows(dep), "macos baseline must accept {dep}");
    }

    let windows = baseline_for("x86_64-pc-windows-msvc").unwrap();
    for dep in ["kernel32.dll", "bcrypt.dll", "api-ms-win-crt-runtime-l1-1-0.dll"] {
        assert!(windows.allows(dep), "windows baseline must accept {dep}");
    }
}

#[test]
fn platform_baselines_reject_third_party_libraries() {
    // The whole point. Each of these is a library a host might supply
    // and which a `*-sys` crate could pick up by accident.
    let linux = baseline_for("x86_64-unknown-linux-gnu").unwrap();
    for dep in [
        "libssl.so.3",
        "libcrypto.so.3",
        "libbpf.so.1",
        "libgit2.so.1.7",
        "libxml2.so.2",
        "libz.so.1",
    ] {
        assert!(!linux.allows(dep), "linux baseline must REJECT {dep}");
    }

    let macos = baseline_for("aarch64-apple-darwin").unwrap();
    for dep in [
        "/opt/homebrew/lib/libssl.3.dylib",
        "/usr/local/lib/libcrypto.3.dylib",
        "@rpath/libgit2.dylib",
    ] {
        assert!(!macos.allows(dep), "macos baseline must REJECT {dep}");
    }

    let windows = baseline_for("x86_64-pc-windows-msvc").unwrap();
    for dep in ["libssl-3-x64.dll", "libcrypto-3-x64.dll", "git2.dll"] {
        assert!(!windows.allows(dep), "windows baseline must REJECT {dep}");
    }
}

#[test]
fn macos_baseline_does_not_accept_a_lookalike_path() {
    // `/usr/local/lib` is Homebrew territory on Intel macs, NOT
    // OS-supplied, and it shares a prefix with nothing in the baseline.
    // Guards against someone "simplifying" the prefix to `/usr/`.
    let macos = baseline_for("aarch64-apple-darwin").unwrap();
    assert!(!macos.allows("/usr/local/lib/libfoo.dylib"));
    assert!(macos.allows("/usr/lib/libSystem.B.dylib"));
}

#[test]
fn unknown_target_has_no_baseline() {
    // A new release target must declare its baseline rather than
    // inheriting an empty allowlist that silently passes.
    assert!(baseline_for("riscv64gc-unknown-linux-gnu").is_none());
    assert!(baseline_for("").is_none());
}

#[test]
fn every_known_target_resolves() {
    for t in known_targets() {
        assert!(baseline_for(t).is_some(), "known target {t} must resolve");
    }
}

#[test]
fn reads_dynamic_deps_from_a_real_binary() {
    // Self-inspection: the test binary is a real Mach-O / ELF / PE for
    // whichever host runs this, so the parser is exercised against a
    // genuine artifact rather than a synthetic fixture. Across the CI
    // matrix that means all three formats are covered — macOS-latest
    // exercises Mach-O, linux-x86_64 exercises ELF, windows-latest
    // exercises PE — without committing a binary fixture.
    //
    // The ELF path was additionally checked by hand against
    // `readelf -d` on a glibc /bin/ls: all three NEEDED entries
    // (libselinux.so.1, libc.so.6, ld-linux-aarch64.so.1) were
    // reported, and libselinux was correctly flagged as outside the
    // baseline.
    let me = std::env::current_exe().unwrap();
    let deps = super::read_dynamic_deps(&me).unwrap();
    assert!(
        !deps.is_empty(),
        "a dynamically-linked test binary should report at least one dependency; \
         an empty result means the parser silently found nothing, which is the \
         failure mode this gate must not have",
    );
}

#[test]
fn rejects_a_file_that_is_not_a_binary() {
    // Failing closed matters: a gate that treats an unreadable artifact
    // as "no dependencies" would pass every release.
    let err = super::read_dynamic_deps_from_bytes(b"#!/bin/sh\necho hello\n");
    assert!(err.is_err(), "a shell script must not parse as a binary");
}
