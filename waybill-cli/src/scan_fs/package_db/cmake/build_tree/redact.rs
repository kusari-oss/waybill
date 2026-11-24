//! Spec 1652 T008 (FR-006, research R8): no host path from a build tree
//! reaches the SBOM. A [`RedactedPath`] is built only by [`Redactor::redact`].

use std::path::{Component, Path, PathBuf};

/// A path safe to emit: `build:<rel>` / `build<N>:<rel>` inside a named build
/// tree, `<rel>` inside the scan root, or `<external>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RedactedPath(String);

impl RedactedPath {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) const EXTERNAL: &str = "<external>";

pub(crate) struct Redactor {
    /// (label, literal root, canonical root), in command-line order.
    trees: Vec<(String, PathBuf, PathBuf)>,
    scan_root: (PathBuf, PathBuf),
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// `rel` with `/` separators and no `.`; empty becomes `.`.
fn slashed(rel: &Path) -> String {
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.is_empty() { ".".to_string() } else { parts.join("/") }
}

impl Redactor {
    pub(crate) fn new(trees: &[PathBuf], scan_root: &Path) -> Self {
        let single = trees.len() == 1;
        let trees = trees
            .iter()
            .enumerate()
            .map(|(i, t)| (if single { "build".to_string() } else { format!("build{}", i + 1) }, t.clone(), canonical(t)))
            .collect();
        Self { trees, scan_root: (scan_root.to_path_buf(), canonical(scan_root)) }
    }

    fn under(p: &Path, literal: &Path, canon: &Path) -> Option<PathBuf> {
        p.strip_prefix(literal).or_else(|_| p.strip_prefix(canon)).ok().map(Path::to_path_buf)
    }

    pub(crate) fn redact(&self, p: &Path) -> RedactedPath {
        // Normalise Windows separators in paths read from build files.
        let owned;
        let p = if p.to_string_lossy().contains('\\') {
            owned = PathBuf::from(p.to_string_lossy().replace('\\', "/"));
            owned.as_path()
        } else {
            p
        };
        let canon = canonical(p);
        for (label, lit, can) in &self.trees {
            if let Some(rel) = Self::under(p, lit, can).or_else(|| Self::under(&canon, lit, can)) {
                return RedactedPath(format!("{label}:{}", slashed(&rel)));
            }
        }
        let (lit, can) = &self.scan_root;
        if let Some(rel) = Self::under(p, lit, can).or_else(|| Self::under(&canon, lit, can)) {
            return RedactedPath(slashed(&rel));
        }
        RedactedPath(EXTERNAL.to_string())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn each_form() {
        let d = tempfile::tempdir().unwrap();
        let (src, b) = (d.path().join("src"), d.path().join("b"));
        std::fs::create_dir_all(src.join("x")).unwrap();
        std::fs::create_dir_all(b.join("_deps/a-src")).unwrap();
        let r = Redactor::new(std::slice::from_ref(&b), &src);
        assert_eq!(r.redact(&b.join("_deps/a-src")).as_str(), "build:_deps/a-src");
        assert_eq!(r.redact(&b).as_str(), "build:.");
        assert_eq!(r.redact(&src.join("x")).as_str(), "x");
        assert_eq!(r.redact(Path::new("/usr/include")).as_str(), EXTERNAL);
    }

    #[test]
    fn several_trees_are_numbered() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let r = Redactor::new(&[a.clone(), b.clone()], d.path());
        assert_eq!(r.redact(&a.join("x")).as_str(), "build1:x");
        assert_eq!(r.redact(&b).as_str(), "build2:.");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_root_redacts_through_either_form() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        std::fs::create_dir_all(real.join("sub")).unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let r = Redactor::new(std::slice::from_ref(&link), d.path().join("src").as_path());
        assert_eq!(r.redact(&real.join("sub")).as_str(), "build:sub");
        assert_eq!(r.redact(&link.join("sub")).as_str(), "build:sub");
    }

    #[test]
    fn backslashes_are_normalised() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path().join("b");
        std::fs::create_dir_all(&b).unwrap();
        let r = Redactor::new(std::slice::from_ref(&b), d.path());
        let windowsish = format!("{}\\_deps\\x-src", b.display());
        assert_eq!(r.redact(Path::new(&windowsish)).as_str(), "build:_deps/x-src");
    }
}
