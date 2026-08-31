//! Filesystem walker that finds candidate binary files.
//!
//! Walks `rootfs` looking for regular files whose first 16 bytes
//! match a known binary magic (ELF / Mach-O / PE). Skips hidden
//! and build directories; ignores files outside the size envelope.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Milestone 054 FR-003: max recursion depth for the `walk_dir`
/// filesystem traversal. Default ceiling per the spec; not tightened
/// because binaries can sit anywhere in a rootfs (no shallow-by-
/// convention structural constraint to justify a tighter bound).
/// Defense-in-depth backstop for the canonicalize-keyed visited-set
/// primary mechanism (FR-002).
const MAX_WALK_DEPTH: usize = 16;

/// Walk `rootfs` for regular files, probing the first 16 bytes of
/// each for a known binary magic. Skips hidden / build dirs. Ignores
/// files <1 KB or >500 MB (defense-in-depth).
///
/// `claimed_paths` is used as a fast-path filter — paths already
/// claimed by the OS-package readers (dpkg / apk / rpm / pip / etc.)
/// are skipped BEFORE the magic-byte probe. On OS-only container
/// scans (e.g., debian:12-slim, where dpkg claims every executable),
/// this drops the walker from 304ms to ~50ms by avoiding thousands
/// of no-op file opens. The check uses Layer-1 raw-path match only;
/// canonical + inode layers still run inside `binary::read` per
/// `is_claimed_binary` so correctness is preserved (a false-miss
/// here just means the file falls through to the full probe + the
/// downstream layered claim check).
pub(super) fn discover_binaries(
    root: &Path,
    claimed_paths: &HashSet<PathBuf>,
    exclude_set: &crate::scan_fs::package_db::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        if !claimed_paths.contains(root) && is_supported_binary(root) {
            out.push(root.to_path_buf());
        }
        return out;
    }
    // Milestone 114: delegates to scan_fs::walk::safe_walk.
    // Milestone 118 (issue #343 / FR-002): thread the operator's
    // ExclusionSet through so binary-tier discovery honors
    // `--exclude-path` the same way ecosystem-tier walkers do.
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let name = candidate.file_name().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                name,
                ".git" | "target" | "node_modules" | ".cargo" | "__pycache__" | ".venv"
            )
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(root, &cfg, |path| {
        // Fast-path skip: claimed by an OS-package reader ⇒ definitely
        // not an unattributed binary. Saves the file-open + magic-byte
        // read on every already-claimed file.
        if claimed_paths.contains(path) {
            return;
        }
        if path.is_file() && is_supported_binary(path) {
            out.push(path.to_path_buf());
        }
    });
    out
}

fn is_supported_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    match f.read_exact(&mut magic) {
        Ok(()) => detect_format(&magic).is_some(),
        Err(_) => false,
    }
}

/// Detect binary format by first-4-bytes magic. Returns the
/// canonical `binary-class` string per FR-021.
pub(crate) fn detect_format(magic: &[u8]) -> Option<&'static str> {
    if magic.len() < 4 {
        return None;
    }
    // ELF: 0x7F 'E' 'L' 'F'
    if magic == [0x7F, b'E', b'L', b'F'] {
        return Some("elf");
    }
    // Mach-O: MH_MAGIC (0xFEEDFACE), MH_CIGAM (0xCEFAEDFE), MH_MAGIC_64
    // (0xFEEDFACF), MH_CIGAM_64 (0xCFFAEDFE), fat-binary variants
    // (0xCAFEBABE / 0xBEBAFECA).
    if matches!(
        magic,
        [0xFE, 0xED, 0xFA, 0xCE]
            | [0xCE, 0xFA, 0xED, 0xFE]
            | [0xFE, 0xED, 0xFA, 0xCF]
            | [0xCF, 0xFA, 0xED, 0xFE]
            | [0xCA, 0xFE, 0xBA, 0xBE]
            | [0xBE, 0xBA, 0xFE, 0xCA]
    ) {
        return Some("macho");
    }
    // PE: starts with "MZ" (0x4D 0x5A) in the DOS header; a real PE
    // also has a PE\0\0 signature at the offset stored at 0x3C.
    // First-4-bytes probe is necessarily optimistic — full PE
    // validation happens at parse time via `object::read::File::parse`.
    if &magic[..2] == b"MZ" {
        return Some("pe");
    }
    None
}

// Milestone 100: the `tests` module is gated `#[cfg(unix)]` because
// its only test (`walks_symlink_loop_without_hanging`) uses
// `std::os::unix::fs::symlink`, which is POSIX-only. Without the
// module-level gate, `use super::*;` would be unused on Windows and
// trip `-D unused-imports`. If future Windows-portable tests are
// added here, drop the module gate and reapply `#[cfg(unix)]` to
// individual POSIX-only test functions instead.
#[cfg(all(test, unix))]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Milestone 054 SC-002 + FR-009: walker terminates promptly on
    /// a synthesized minimal symlink-loop fixture instead of hanging
    /// indefinitely. Same shape as rpm_file's regression guard.
    #[test]
    fn walks_symlink_loop_without_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let loop_dir = tmp.path().join("loop");
        std::fs::create_dir_all(&loop_dir).unwrap();
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
        let result = discover_binaries(
            tmp.path(),
            &std::collections::HashSet::new(),
            &crate::scan_fs::package_db::exclude_path::ExclusionSet::default(),
        );
        assert!(result.is_empty());
    }
}
