#![allow(dead_code)] // US1.A scaffolding; US1.B wires every entry point in.

//! Milestone 133 US1 — content-shape allowlist per FR-005.
//!
//! Files surviving [`classify`] qualify for file-tier emission;
//! every other file is silently skipped (the SBOM doesn't grow). The
//! exclusion list is tighter than a naïve "every binary file": source
//! code / docs / configs are explicitly excluded by extension, AND
//! known package-install path prefixes (`**/node_modules/**`,
//! `**/.cargo/registry/**`, etc.) are stripped via [`build_orphan_exclusion_globs`]
//! because those directories ARE the package-tier reader's domain —
//! we don't double-emit content the package-DB reader already
//! claims.
//!
//! **Allowlist** (FR-005, tightened per FR-022 projection): ELF /
//! PE / Mach-O binary magic; shared libs by extension; archives by
//! extension; lone manifests WITH no adjacent lockfile; exec
//! scripts.
//!
//! **Hard exclusion** (FR-005): source-code / plain-text /
//! structured-config extensions, plus the path-prefix list captured
//! in [`ORPHAN_PATH_EXCLUSIONS`].

use std::path::Path;

use globset::{Glob, GlobSetBuilder};

/// FR-005 content-shape classification for a single file. Variants
/// are mutually exclusive — the classifier picks the first matching
/// shape and returns; downstream emission keys off the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentShape {
    /// ELF executable / shared object — magic bytes `\x7FELF`.
    ElfBinary,
    /// Windows Portable Executable — `MZ\x90\x00` (DOS stub at file head).
    PeBinary,
    /// Mach-O — 32-bit `\xFE\xED\xFA\xCE` or 64-bit `\xFE\xED\xFA\xCF`
    /// (also handles the reverse-endian variants `\xCE\xFA\xED\xFE`
    /// and `\xCF\xFA\xED\xFE`).
    MachoBinary,
    /// `.so` / `.dylib` / `.dll` by extension when magic didn't classify
    /// as one of the binary variants above (e.g. an empty/stub `.so`).
    SharedLib,
    /// `.jar` / `.war` / `.ear` / generic `.zip` not already opened by
    /// mikebom's archive walkers.
    JavaOrArchive,
    /// `.deb` / `.rpm` / `.apk` not already opened by the OS-package
    /// readers.
    OsPackage,
    /// `.tar.gz` / `.tgz` / `.tar.xz` / `.tar.bz2` / `.tar` not already
    /// opened by mikebom's archive walkers.
    CompressedArchive,
    /// Lone package manifest with no adjacent lockfile — `package.json`
    /// without sibling `package-lock.json` etc. The adjacent-lockfile
    /// check is performed inside [`classify`] before this variant
    /// returns.
    LoneManifest,
    /// Any file with `#!` magic in its first two bytes (executable bit
    /// not enforced — image rootfs scans frequently lose POSIX perms in
    /// transit, and a shebang is signal enough).
    ExecScript,
}

/// FR-005 path-prefix exclusion list. Known package install roots
/// where the package-tier reader knows the package identity via
/// PURL but hasn't yet been extended to emit per-file path coverage.
/// These directories' contents are silently skipped by [`classify`]
/// regardless of content shape — pragmatic stop-gap until US2
/// reader expansion (milestone 134+) fills in `evidence.occurrences[]`
/// for those readers.
///
/// Glob-matched via `globset::GlobSet`. The leading `**/` makes
/// each pattern rootfs-anchored — accepts e.g. `usr/share/dotnet/...`
/// AND `opt/some-app/usr/share/dotnet/...`.
pub(crate) const ORPHAN_PATH_EXCLUSIONS: &[&str] = &[
    "**/dotnet/packs/**",
    "**/dotnet/shared/**",
    "**/dotnet/sdk/**",
    "**/dotnet/store/**",
    "**/usr/share/dotnet/**",
    "**/node_modules/**",
    "**/lib/python*/site-packages/**",
    "**/.cargo/registry/**",
    "**/ruby/gems/**",
    "**/jvm/openjdk*/lib/**",
];

/// FR-005 content-shape EXCLUSION list — extensions of files that
/// MUST NOT be emitted as file-tier components regardless of any
/// other classification. Matches case-insensitively against the final
/// component of the file's name.
const EXCLUDED_EXTENSIONS: &[&str] = &[
    // Source code
    "rs", "py", "go", "c", "cpp", "cc", "cxx", "h", "hpp", "cs", "java", "js", "ts", "jsx", "tsx",
    "rb", "php", "swift", "kt", "kts", "scala", "clj", "ex", "exs", "erl", "lua", "pl", "pm",
    // Plain text / docs
    "md", "txt", "rst", "adoc", "asciidoc", "tex",
    // Structured config
    "json", "yaml", "yml", "toml", "ini", "conf", "cfg",
    // XML — *only* when it's not one of the known archive shapes (jar/war/ear are
    // ZIP-archives and their classification is by-extension above, NOT here, so
    // raw `.xml` falls through to this exclusion).
    "xml",
    // Build configs (separate set to make intent clear in code)
    "Dockerfile", "Makefile", "Rakefile", "Gemfile",
    // CI/scaffolding
    "lock", "sum", "list",
];

/// Lone-manifest filenames that qualify for FR-005's lone-manifest
/// classification IF and only if the FR-005 adjacent-lockfile check
/// at the call site fails to find a sibling lockfile.
const LONE_MANIFEST_FILENAMES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pom.xml",
    "requirements.txt",
    "Gemfile",
    "go.mod",
];

/// Lockfile filenames whose presence in the same directory (or for
/// `Cargo.lock`, any parent up to the workspace root) disqualifies
/// the manifest from `LoneManifest` classification.
const SIBLING_LOCKFILES_BY_MANIFEST: &[(&str, &[&str])] = &[
    (
        "package.json",
        &["package-lock.json", "yarn.lock", "pnpm-lock.yaml"],
    ),
    ("Cargo.toml", &["Cargo.lock"]),
    ("requirements.txt", &["requirements-freeze.txt", "pyproject.toml"]),
    ("Gemfile", &["Gemfile.lock"]),
    ("go.mod", &["go.sum"]),
];

/// Build the path-prefix exclusion `GlobSet` once per scan. Failure
/// to compile any pattern returns an empty `GlobSet` — fail-open
/// here because the patterns are checked-in constants and a build
/// failure at runtime would silently disable the exclusion list.
/// We log via `tracing::warn!` instead, then return the empty set.
pub(crate) fn build_orphan_exclusion_globs() -> globset::GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pat in ORPHAN_PATH_EXCLUSIONS {
        match Glob::new(pat) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(err) => {
                tracing::warn!(
                    pattern = pat,
                    error = %err,
                    "orphan-exclusion glob failed to compile; entry skipped"
                );
            }
        }
    }
    builder.build().unwrap_or_else(|err| {
        tracing::warn!(
            error = %err,
            "orphan-exclusion GlobSet failed to build; falling back to empty set"
        );
        globset::GlobSet::empty()
    })
}

/// FR-005 classifier. Returns `Some(ContentShape)` when the file
/// qualifies for file-tier emission; `None` when it should be
/// skipped (excluded by extension, by path prefix, or by absence of
/// any matching shape).
///
/// The classifier is a PURE FUNCTION (no I/O on `rel_path`,
/// `rootfs_root`, or `exclusion_globs`); the SHA-256 hashing and
/// adjacent-lockfile filesystem probes happen in the caller
/// (`walker.rs`). This keeps the function unit-testable without
/// touching the filesystem and lets the walker batch I/O.
pub(crate) fn classify(
    rel_path: &Path,
    head_bytes: &[u8],
    exclusion_globs: &globset::GlobSet,
    lockfile_check: impl FnOnce() -> bool,
) -> Option<ContentShape> {
    // 1. Path-prefix exclusion list. Anything under `**/node_modules/**`
    //    or `**/.cargo/registry/**` etc. is package-tier territory —
    //    silently skip regardless of shape.
    if exclusion_globs.is_match(rel_path) {
        return None;
    }

    // 2. Extension-based hard exclusion (source code / docs / configs).
    //    Compare case-insensitively against the file's extension.
    let lower_filename = rel_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    if let Some(name) = lower_filename.as_deref() {
        if EXCLUDED_EXTENSIONS.iter().any(|ext| {
            // Match full filename for extensionless build configs
            // (Dockerfile, Makefile, Rakefile, Gemfile).
            ext.eq_ignore_ascii_case(name) ||
            // Match dot-extension for source / docs / configs.
            (name.ends_with(&format!(".{}", ext.to_ascii_lowercase())))
        }) {
            // Special case: lone manifests are excluded from the
            // hard exclusion (we WANT to classify them as
            // `LoneManifest` when there's no adjacent lockfile).
            // Check separately below.
            let is_lone_candidate = rel_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| LONE_MANIFEST_FILENAMES.contains(&s))
                .unwrap_or(false);
            if !is_lone_candidate {
                return None;
            }
        }
    }

    // 3. Magic-number probe (highest signal): ELF / PE / Mach-O.
    if let Some(shape) = magic_classify(head_bytes) {
        return Some(shape);
    }

    // 4. Extension-based positive classification when magic didn't fire.
    if let Some(name) = lower_filename.as_deref() {
        if name.ends_with(".so")
            || name.contains(".so.") // versioned (libfoo.so.1, .so.1.2.3)
            || name.ends_with(".dylib")
            || name.ends_with(".dll")
        {
            return Some(ContentShape::SharedLib);
        }
        if name.ends_with(".jar")
            || name.ends_with(".war")
            || name.ends_with(".ear")
            || name.ends_with(".zip")
        {
            return Some(ContentShape::JavaOrArchive);
        }
        if name.ends_with(".deb") || name.ends_with(".rpm") || name.ends_with(".apk") {
            return Some(ContentShape::OsPackage);
        }
        if name.ends_with(".tar.gz")
            || name.ends_with(".tgz")
            || name.ends_with(".tar.xz")
            || name.ends_with(".tar.bz2")
            || name.ends_with(".tar")
        {
            return Some(ContentShape::CompressedArchive);
        }
    }

    // 5. Lone manifest with FR-005 adjacent-lockfile check.
    let is_lone_candidate = rel_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| LONE_MANIFEST_FILENAMES.contains(&s))
        .unwrap_or(false);
    if is_lone_candidate {
        // Caller's closure performs the I/O-bound sibling check;
        // returns `true` when a lockfile is found (NOT lone).
        if !lockfile_check() {
            return Some(ContentShape::LoneManifest);
        }
        return None;
    }

    // 6. Executable script: first two bytes `#!`.
    if head_bytes.len() >= 2 && head_bytes[0] == b'#' && head_bytes[1] == b'!' {
        return Some(ContentShape::ExecScript);
    }

    // 7. No match — silently skip.
    None
}

/// Return the lockfile names that disqualify a given manifest name
/// from `LoneManifest` classification. Used by the walker to drive
/// the adjacent-lockfile probe. Empty slice for manifests with no
/// FR-005 sibling-lockfile rule.
pub(crate) fn sibling_lockfiles_for(manifest_filename: &str) -> &'static [&'static str] {
    for (mname, locks) in SIBLING_LOCKFILES_BY_MANIFEST {
        if *mname == manifest_filename {
            return locks;
        }
    }
    &[]
}

/// `pom.xml` is special: there's no Maven lockfile, but the
/// presence of a `target/` build-output sibling means "real build,
/// not vendored source-tree signal". Caller probes for the
/// directory; this helper just exposes the name string.
pub(crate) const POM_BUILD_OUTPUT_DIR: &str = "target";

fn magic_classify(head: &[u8]) -> Option<ContentShape> {
    if head.len() >= 4 && &head[0..4] == b"\x7FELF" {
        return Some(ContentShape::ElfBinary);
    }
    if head.len() >= 2 && &head[0..2] == b"MZ" {
        return Some(ContentShape::PeBinary);
    }
    if head.len() >= 4 {
        let m = &head[0..4];
        if m == b"\xFE\xED\xFA\xCE"
            || m == b"\xFE\xED\xFA\xCF"
            || m == b"\xCE\xFA\xED\xFE"
            || m == b"\xCF\xFA\xED\xFE"
        {
            return Some(ContentShape::MachoBinary);
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn empty_globs() -> globset::GlobSet {
        globset::GlobSet::empty()
    }

    #[test]
    fn elf_binary_magic_classifies() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("opt/custom/tool"),
            b"\x7FELF\x02\x01\x01\x00",
            &g,
            || false,
        );
        assert_eq!(shape, Some(ContentShape::ElfBinary));
    }

    #[test]
    fn pe_binary_magic_classifies() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("opt/tools/foo.exe"),
            b"MZ\x90\x00\x03\x00\x00\x00",
            &g,
            || false,
        );
        assert_eq!(shape, Some(ContentShape::PeBinary));
    }

    #[test]
    fn macho_binary_magic_classifies() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("usr/local/bin/foo"),
            b"\xFE\xED\xFA\xCF\x07\x00\x00\x01",
            &g,
            || false,
        );
        assert_eq!(shape, Some(ContentShape::MachoBinary));
    }

    #[test]
    fn versioned_shared_lib_classifies() {
        let g = empty_globs();
        let shape = classify(&PathBuf::from("usr/lib/libfoo.so.1.2"), &[], &g, || false);
        assert_eq!(shape, Some(ContentShape::SharedLib));
    }

    #[test]
    fn rust_source_file_skipped() {
        let g = empty_globs();
        let shape = classify(&PathBuf::from("app/src/main.rs"), b"fn main()", &g, || false);
        assert_eq!(shape, None);
    }

    #[test]
    fn yaml_config_skipped() {
        let g = empty_globs();
        let shape = classify(&PathBuf::from("etc/foo.yaml"), b"a:", &g, || false);
        assert_eq!(shape, None);
    }

    #[test]
    fn lone_manifest_no_adjacent_lockfile_classifies() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("app/Cargo.toml"),
            b"[package]",
            &g,
            || false, // no lockfile
        );
        assert_eq!(shape, Some(ContentShape::LoneManifest));
    }

    #[test]
    fn manifest_with_adjacent_lockfile_skipped() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("app/Cargo.toml"),
            b"[package]",
            &g,
            || true, // lockfile present
        );
        assert_eq!(shape, None);
    }

    #[test]
    fn shebang_script_classifies() {
        let g = empty_globs();
        let shape = classify(&PathBuf::from("usr/local/bin/run"), b"#!/bin/sh\n", &g, || {
            false
        });
        assert_eq!(shape, Some(ContentShape::ExecScript));
    }

    #[test]
    fn jar_classifies_as_archive() {
        let g = empty_globs();
        let shape = classify(&PathBuf::from("opt/app.jar"), &[], &g, || false);
        assert_eq!(shape, Some(ContentShape::JavaOrArchive));
    }

    #[test]
    fn deb_classifies_as_os_package() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("tmp/foo_1.0_amd64.deb"),
            &[],
            &g,
            || false,
        );
        assert_eq!(shape, Some(ContentShape::OsPackage));
    }

    #[test]
    fn tarball_classifies_as_compressed_archive() {
        let g = empty_globs();
        let shape = classify(
            &PathBuf::from("opt/foo-1.0.tar.gz"),
            &[],
            &g,
            || false,
        );
        assert_eq!(shape, Some(ContentShape::CompressedArchive));
    }

    #[test]
    fn path_prefix_exclusion_skips_dotnet_packs() {
        let globs = build_orphan_exclusion_globs();
        // ELF magic + path under dotnet/packs/ → skip.
        let shape = classify(
            &PathBuf::from("usr/share/dotnet/packs/Microsoft.NETCore.App.Runtime.linux-musl-x64/8.0.27/runtimes/linux-musl-x64/lib/net8.0/System.Diagnostics.Tools.dll"),
            b"MZ\x90\x00",
            &globs,
            || false,
        );
        assert_eq!(shape, None);
    }

    #[test]
    fn path_prefix_exclusion_skips_node_modules() {
        let globs = build_orphan_exclusion_globs();
        let shape = classify(
            &PathBuf::from("app/node_modules/.bin/express"),
            b"\x7FELF",
            &globs,
            || false,
        );
        assert_eq!(shape, None);
    }

    #[test]
    fn path_prefix_exclusion_skips_cargo_registry() {
        let globs = build_orphan_exclusion_globs();
        let shape = classify(
            &PathBuf::from("root/.cargo/registry/src/index.crates.io/serde-1.0.0/Cargo.toml"),
            b"[package]",
            &globs,
            || false,
        );
        assert_eq!(shape, None);
    }

    #[test]
    fn sibling_lockfile_lookup_for_known_manifest_returns_nonempty_slice() {
        assert_eq!(
            sibling_lockfiles_for("package.json"),
            &["package-lock.json", "yarn.lock", "pnpm-lock.yaml"]
        );
        assert_eq!(sibling_lockfiles_for("Cargo.toml"), &["Cargo.lock"]);
        assert_eq!(sibling_lockfiles_for("Gemfile"), &["Gemfile.lock"]);
    }

    #[test]
    fn sibling_lockfile_lookup_for_unknown_manifest_returns_empty_slice() {
        assert!(sibling_lockfiles_for("notarealmanifest.toml").is_empty());
    }

    #[test]
    fn build_orphan_exclusion_globs_compiles_all_patterns() {
        let g = build_orphan_exclusion_globs();
        assert!(g.is_match("foo/dotnet/packs/bar"));
        assert!(g.is_match("foo/node_modules/baz"));
        assert!(g.is_match("foo/lib/python3.11/site-packages/qux"));
    }
}
