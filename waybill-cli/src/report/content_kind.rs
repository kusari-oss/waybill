//! Milestone 924 (#932) — FR-011: binary vs text, as observation.
//!
//! 47 binary files and 47 UTF-8 text files are both unclassified and mean
//! entirely different things — "probably build output" versus "probably source
//! in a language we do not read". This field is the difference between a
//! report that says "unknown" and one that is actionable.
//!
//! **A heuristic, reported as such** (research R4). NUL byte in the sampled
//! prefix ⇒ binary; otherwise valid UTF-8 ⇒ text; otherwise binary. The sample
//! bound travels with the verdict so a reader knows it came from a prefix and
//! not the whole file. No new dependency: `std::str::from_utf8` decides it.

use std::path::Path;

/// Bytes sampled per file. Large enough to clear any plausible text preamble
/// in a binary, small enough that sampling a large directory is cheap.
pub(crate) const SAMPLE_BYTES: u32 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentKind {
    PredominantlyBinary,
    PredominantlyText,
    Mixed,
    Empty,
}

impl ContentKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PredominantlyBinary => "predominantly_binary",
            Self::PredominantlyText => "predominantly_text",
            Self::Mixed => "mixed",
            Self::Empty => "empty",
        }
    }
}

/// Classify one file from its sampled prefix. `None` when unreadable — an
/// unreadable file is not evidence of either kind, and guessing would be the
/// sort of assertion FR-014 forbids.
fn file_is_text(path: &Path) -> Option<bool> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; SAMPLE_BYTES as usize];
    let n = f.read(&mut buf).ok()?;
    let head = &buf[..n];
    if head.is_empty() {
        return Some(true); // an empty file is vacuously text
    }
    if head.contains(&0) {
        return Some(false);
    }
    // A truncated multi-byte character at the sample boundary is not evidence
    // of binary content, so a trailing incomplete sequence is tolerated.
    Some(match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && e.valid_up_to() > 0,
    })
}

/// Classify a directory from the files directly in it.
pub(crate) fn classify_dir(dir: &Path, filenames: &std::collections::BTreeSet<String>)
    -> ContentKind
{
    let (mut text, mut binary) = (0usize, 0usize);
    for name in filenames {
        match file_is_text(&dir.join(name)) {
            Some(true) => text += 1,
            Some(false) => binary += 1,
            None => {}
        }
    }
    match (text, binary) {
        (0, 0) => ContentKind::Empty,
        (t, 0) if t > 0 => ContentKind::PredominantlyText,
        (0, b) if b > 0 => ContentKind::PredominantlyBinary,
        (t, b) if t >= b * 4 => ContentKind::PredominantlyText,
        (t, b) if b >= t * 4 => ContentKind::PredominantlyBinary,
        _ => ContentKind::Mixed,
    }
}

/// Extension histogram. **Observation only** — FR-008 forbids it driving an
/// attribution, and it is recorded here precisely so a reader can draw their
/// own conclusion where the report declines to.
pub(crate) fn extension_histogram(filenames: &std::collections::BTreeSet<String>)
    -> std::collections::BTreeMap<String, u64>
{
    let mut h = std::collections::BTreeMap::new();
    for name in filenames {
        let ext = Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "<none>".to_string());
        *h.entry(ext).or_insert(0u64) += 1;
    }
    h
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn names(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_nul_byte_means_binary() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.bin"), b"ELF\0\0\0data").unwrap();
        assert_eq!(classify_dir(d.path(), &names(&["a.bin"])), ContentKind::PredominantlyBinary);
    }

    #[test]
    fn valid_utf8_means_text() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.txt"), "hello — world\n").unwrap();
        assert_eq!(classify_dir(d.path(), &names(&["a.txt"])), ContentKind::PredominantlyText);
    }

    /// The discrimination that makes an unclassified directory actionable.
    #[test]
    fn equal_sized_binary_and_text_directories_are_distinguishable() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path().join("b");
        let t = d.path().join("t");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&t).unwrap();
        for i in 0..5 {
            std::fs::write(b.join(format!("f{i}")), b"\0\0binary\0").unwrap();
            std::fs::write(t.join(format!("f{i}")), b"plain text\n").unwrap();
        }
        let n = names(&["f0", "f1", "f2", "f3", "f4"]);
        assert_eq!(classify_dir(&b, &n), ContentKind::PredominantlyBinary);
        assert_eq!(classify_dir(&t, &n), ContentKind::PredominantlyText);
    }

    #[test]
    fn a_truncated_multibyte_char_at_the_sample_edge_is_not_binary() {
        let d = tempfile::tempdir().unwrap();
        // Fill past the sample bound so the prefix ends mid-character.
        let mut body = "a".repeat(SAMPLE_BYTES as usize - 1);
        body.push('é');
        std::fs::write(d.path().join("a.txt"), body.as_bytes()).unwrap();
        assert_eq!(classify_dir(d.path(), &names(&["a.txt"])), ContentKind::PredominantlyText);
    }

    #[test]
    fn the_histogram_counts_extensions_and_names_the_extensionless() {
        let h = extension_histogram(&names(&["a.rs", "b.rs", "Makefile", "c.TXT"]));
        assert_eq!(h.get("rs"), Some(&2));
        assert_eq!(h.get("txt"), Some(&1), "extensions are lowercased");
        assert_eq!(h.get("<none>"), Some(&1));
    }
}
