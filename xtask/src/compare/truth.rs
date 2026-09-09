// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Truth-set derivation (FR-008a, FR-008b).
//
// Which method produced a truth set is recorded with every score computed
// from it, and scores from different methods are never compared. No method
// is authoritative globally: each target declares its own.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;

use super::config::TruthMethod;
use super::identity::{parse_purl, PackageIdentity};

/// A derived truth set plus the method that produced it.
#[derive(Debug, Clone)]
pub struct TruthSet {
    pub method: TruthMethod,
    pub identities: BTreeSet<PackageIdentity>,
}

impl TruthSet {
    pub fn len(&self) -> usize {
        self.identities.len()
    }
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }
    /// FR-008b — a superset changes what `found` and `extra` mean.
    pub fn is_superset(&self) -> bool {
        self.method.is_superset()
    }
}

pub fn derive(method: TruthMethod, root: &Path) -> Result<TruthSet, Box<dyn Error>> {
    let identities = match method {
        TruthMethod::GoSumUnion => go_sum_union(root)?,
        TruthMethod::GoModRequires => go_mod_requires(root)?,
        TruthMethod::DeclaredExact => declared_exact(root)?,
    };
    Ok(TruthSet { method, identities })
}

/// Walk the tree collecting files named `name`. Skips nothing: a nested
/// module's manifest is as real as the root's.
fn find_files(root: &Path, name: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Union of every `go.sum`. Always available offline.
///
/// **This is a SUPERSET of what is built.** `go.sum` carries hashes for
/// modules merely considered during resolution, including ones never
/// linked. Scoring against it rewards a tool for reporting more and
/// penalises one that correctly omits what the build does not use.
fn go_sum_union(root: &Path) -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
    let mut out = BTreeSet::new();
    for file in find_files(root, "go.sum") {
        let text = std::fs::read_to_string(&file)?;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(module), Some(version)) = (parts.next(), parts.next()) else {
                continue;
            };
            // `<module> <version>/go.mod <hash>` duplicates the module line;
            // both reduce to the same identity, which is correct.
            let version = version.trim_end_matches("/go.mod");
            if module.is_empty() || version.is_empty() {
                continue;
            }
            if let Some(id) = parse_purl(&format!("pkg:golang/{module}@{version}")) {
                out.insert(id);
            }
        }
    }
    Ok(out)
}

/// Union of every `go.mod` require directive. Closer to the built set than
/// `go.sum`, still not exact.
fn go_mod_requires(root: &Path) -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
    let mut out = BTreeSet::new();
    for file in find_files(root, "go.mod") {
        let text = std::fs::read_to_string(&file)?;
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("").trim();
            let line = line.strip_prefix("require ").unwrap_or(line);
            let mut parts = line.split_whitespace();
            let (Some(module), Some(version)) = (parts.next(), parts.next()) else {
                continue;
            };
            if !module.contains('.') || !version.starts_with('v') {
                continue;
            }
            if let Some(id) = parse_purl(&format!("pkg:golang/{module}@{version}")) {
                out.insert(id);
            }
        }
    }
    Ok(out)
}

/// A committed `EXPECTED.txt`, one identity per line, `#` for comments.
/// Exact by construction.
fn declared_exact(root: &Path) -> Result<BTreeSet<PackageIdentity>, Box<dyn Error>> {
    let path = root.join("EXPECTED.txt");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("declared-exact truth needs {}: {e}", path.display()))?;
    let mut out = BTreeSet::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let id = parse_purl(line)
            .ok_or_else(|| format!("{}:{}: not a usable purl: {line}", path.display(), n + 1))?;
        out.insert(id);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("compare/fixtures/known-answer-go")
    }

    #[test]
    fn go_sum_union_recovers_the_fixture_set() {
        let t = derive(TruthMethod::GoSumUnion, &fixture()).expect("derive");
        assert_eq!(t.len(), 5, "five distinct module-version pairs");
        assert!(t.is_superset(), "go.sum is a superset and must say so");
    }

    #[test]
    fn go_sum_union_keeps_both_versions_of_one_module() {
        let t = derive(TruthMethod::GoSumUnion, &fixture()).expect("derive");
        let multi: Vec<_> = t
            .identities
            .iter()
            .filter(|i| i.name == "waybill-fixture-multi")
            .collect();
        assert_eq!(multi.len(), 2, "v1.0.0 and v2.0.0 are distinct identities");
    }

    #[test]
    fn declared_exact_matches_go_sum_union_on_the_fixture() {
        // The fixture is built so both methods agree. If they diverge, one
        // of the two derivations has drifted.
        let a = derive(TruthMethod::GoSumUnion, &fixture()).expect("go.sum");
        let b = derive(TruthMethod::DeclaredExact, &fixture()).expect("declared");
        assert_eq!(a.identities, b.identities);
        assert!(!b.is_superset(), "declared-exact is exact by construction");
    }

    #[test]
    fn go_mod_requires_omits_what_go_sum_includes() {
        // Demonstrates the methods are genuinely different, which is why
        // scores from them are never compared (FR-008a).
        let sum = derive(TruthMethod::GoSumUnion, &fixture()).expect("go.sum");
        let md = derive(TruthMethod::GoModRequires, &fixture()).expect("go.mod");
        assert!(md.len() < sum.len(), "go.mod requires is the smaller set");
    }
}
