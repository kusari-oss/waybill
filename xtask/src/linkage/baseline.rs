// Issue #824 — the per-target platform baseline.
//
// Constitution v3.0.0 enumerates these rather than describing them,
// deliberately: waybill's macOS binary already links libiconv,
// CoreFoundation and CoreServices, so a vague boundary would have left
// the shipping product ambiguously non-compliant the day the amendment
// merged. The constitution is the source of truth; this file is its
// executable form and must not drift from it.
//
// Rule for editing: an entry belongs here only if the operating system
// itself ships and guarantees it. A third-party library that happens to
// be installed on the build machine does not qualify — that is exactly
// the failure this gate exists to catch.

/// Matcher for one target's permitted dynamic dependencies.
pub struct Baseline {
    exact: &'static [&'static str],
    prefixes: &'static [&'static str],
}

impl Baseline {
    pub fn allows(&self, dep: &str) -> bool {
        if self.exact.iter().any(|e| e.eq_ignore_ascii_case(dep)) {
            return true;
        }
        self.prefixes
            .iter()
            .any(|p| dep.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase()))
    }
}

/// Linux glibc targets. `libc`, `libm`, `libpthread`, `libdl`, `librt`
/// and the dynamic loader, per Principle I. glibc 2.34+ folds
/// libpthread/libdl/librt into libc, so those appear only on older
/// toolchains — listed because `cross` images vary.
const LINUX_GNU: Baseline = Baseline {
    exact: &[
        "libc.so.6",
        "libm.so.6",
        "libpthread.so.0",
        "libdl.so.2",
        "librt.so.1",
        "libgcc_s.so.1",
        "ld-linux-x86-64.so.2",
        "ld-linux-aarch64.so.1",
    ],
    prefixes: &[],
};

/// macOS. libSystem plus OS-supplied dylibs under /usr/lib and
/// frameworks under /System/Library. The prefix form is used here
/// because the loader records absolute paths and Apple ships a long
/// tail of them; the path prefix IS the guarantee that the OS owns it.
const MACOS: Baseline = Baseline {
    exact: &[],
    prefixes: &["/usr/lib/", "/System/Library/Frameworks/"],
};

/// Windows MSVC. The Universal CRT and System32 DLLs. PE imports are
/// recorded as bare filenames, so these are matched exactly; the UCRT
/// is split across `api-ms-win-crt-*` stubs, hence the prefix.
const WINDOWS_MSVC: Baseline = Baseline {
    exact: &[
        "kernel32.dll",
        "advapi32.dll",
        "ntdll.dll",
        "bcrypt.dll",
        "bcryptprimitives.dll",
        "crypt32.dll",
        "secur32.dll",
        "ws2_32.dll",
        "userenv.dll",
        "user32.dll",
        "ole32.dll",
        "oleaut32.dll",
        "shell32.dll",
        "psapi.dll",
        "powrprof.dll",
        "vcruntime140.dll",
        "msvcrt.dll",
        "ntoskrnl.exe",
    ],
    prefixes: &["api-ms-win-"],
};

pub fn baseline_for(target: &str) -> Option<Baseline> {
    match target {
        "x86_64-unknown-linux-gnu" | "aarch64-unknown-linux-gnu" => Some(LINUX_GNU),
        "aarch64-apple-darwin" | "x86_64-apple-darwin" => Some(MACOS),
        "x86_64-pc-windows-msvc" => Some(WINDOWS_MSVC),
        _ => None,
    }
}

pub fn known_targets() -> Vec<&'static str> {
    vec![
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ]
}
