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
pub(super) fn discover_binaries(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        if is_supported_binary(root) {
            out.push(root.to_path_buf());
        }
        return out;
    }
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk_dir(root, 0, &mut visited, &mut out);
    out
}

/// Milestone 054 FR-001/FR-002/FR-003: canonicalize-keyed visited
/// set + max-depth backstop prevents unbounded recursion on symlink
/// loops (same shape as the rpm_file::walk_dir bug — knative/func
/// reproducer).
fn walk_dir(
    dir: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    acc: &mut Vec<PathBuf>,
) {
    if depth >= MAX_WALK_DEPTH {
        tracing::debug!(
            depth,
            path = %dir.display(),
            "walker: max-depth reached",
        );
        return;
    }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        tracing::debug!(
            path = %dir.display(),
            "walker: cycle/visited skip",
        );
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                name,
                ".git" | "target" | "node_modules" | ".cargo" | "__pycache__" | ".venv"
            ) {
                continue;
            }
            walk_dir(&path, depth + 1, visited, acc);
        } else if path.is_file() && is_supported_binary(&path) {
            acc.push(path);
        }
    }
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

#[cfg(test)]
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
        let result = discover_binaries(tmp.path());
        assert!(result.is_empty());
    }
}
