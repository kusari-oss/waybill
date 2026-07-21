//! In-process per-scan registry that maps a (library-name,
//! binary-path) pair to the cmake `CmakeBuildDirObservation` that
//! drives PURL attribution.
//!
//! Lookup contract (per `contracts/attribution-rules.md`):
//! a `SymbolFingerprintMatch.library` (lowercased) joins to an
//! observation when the binary being scanned has the observation's
//! `cmake_project_build_root` as a path-ancestor. Multiple
//! observations under the same library key resolve to the closest
//! path-ancestor of the binary; ties broken lexically + `tracing::warn!`.

use std::collections::BTreeMap;
use std::path::Path;

use super::CmakeBuildDirObservation;

#[allow(dead_code)]
pub(crate) struct BuildAttributionRegistry {
    by_library_lc: BTreeMap<String, Vec<CmakeBuildDirObservation>>,
}

#[allow(dead_code)]
impl BuildAttributionRegistry {
    /// Build from a flat list of observations. Each observation's
    /// `library_name` is lowercased to form the lookup key.
    pub(crate) fn from_observations(
        observations: Vec<CmakeBuildDirObservation>,
    ) -> Self {
        let mut by_library_lc: BTreeMap<String, Vec<CmakeBuildDirObservation>> =
            BTreeMap::new();
        for obs in observations {
            let key = obs.library_name.to_ascii_lowercase();
            by_library_lc.entry(key).or_default().push(obs);
        }
        Self { by_library_lc }
    }

    /// True when the registry has no observations (the common case
    /// for non-cmake-project scans). Callers can skip the per-match
    /// lookup loop when this returns true.
    pub(crate) fn is_empty(&self) -> bool {
        self.by_library_lc.is_empty()
    }

    /// Look up an attribution for the given `library` (case-
    /// insensitive) and `binary_path`. Returns `Some(observation)`
    /// when (a) the lowercased library is a key, AND (b) at least
    /// one observation under that key has a `cmake_project_build_root`
    /// that's a path-ancestor of `binary_path`.
    ///
    /// When multiple observations are candidates (multi-cmake-project
    /// workspace), pick the one whose `cmake_project_build_root` is
    /// the CLOSEST path-ancestor of `binary_path` (deepest matching
    /// ancestor). On ancestry-depth ties, pick deterministically by
    /// lexical sort of the build-root path + emit a `tracing::warn!`
    /// so operators with pathological workspace layouts see the
    /// ambiguity.
    pub(crate) fn lookup(
        &self,
        library: &str,
        binary_path: &Path,
    ) -> Option<&CmakeBuildDirObservation> {
        let key = library.to_ascii_lowercase();
        let candidates = self.by_library_lc.get(&key)?;
        // Filter to observations whose project root is a path-ancestor
        // of binary_path. Then pick the deepest (longest) ancestor.
        let mut hits: Vec<&CmakeBuildDirObservation> = candidates
            .iter()
            .filter(|obs| is_path_ancestor(&obs.cmake_project_build_root, binary_path))
            .collect();
        if hits.is_empty() {
            return None;
        }
        // Sort: deeper ancestor first (longer path string ≈ deeper
        // in the tree for well-formed paths). Tie-break lexically
        // for determinism.
        hits.sort_by(|a, b| {
            let alen = a.cmake_project_build_root.as_os_str().len();
            let blen = b.cmake_project_build_root.as_os_str().len();
            blen.cmp(&alen).then_with(|| {
                a.cmake_project_build_root
                    .as_os_str()
                    .cmp(b.cmake_project_build_root.as_os_str())
            })
        });
        // Detect ambiguity: more than one hit at the same depth.
        if hits.len() > 1
            && hits[0].cmake_project_build_root.as_os_str().len()
                == hits[1].cmake_project_build_root.as_os_str().len()
        {
            tracing::warn!(
                library = %library,
                binary = %binary_path.display(),
                chosen = %hits[0].cmake_project_build_root.display(),
                "multiple cmake observations match {library} for {binary}; \
                 picking {chosen} (lexically first at this depth)",
                library = library,
                binary = binary_path.display(),
                chosen = hits[0].cmake_project_build_root.display(),
            );
        }
        Some(hits[0])
    }
}

/// True when `ancestor` is a path-ancestor of `descendant` (or
/// equal). Pure path comparison; does not touch the filesystem.
fn is_path_ancestor(ancestor: &Path, descendant: &Path) -> bool {
    let mut cur = descendant;
    loop {
        if cur == ancestor {
            return true;
        }
        match cur.parent() {
            Some(parent) if parent != cur => cur = parent,
            _ => return false,
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn obs(library_name: &str, project_root: &str) -> CmakeBuildDirObservation {
        CmakeBuildDirObservation {
            library_name: library_name.to_string(),
            source_tier_purl: format!("pkg:github/example/{library_name}@v1.0"),
            source_mechanism: "cmake-fetchcontent-git".to_string(),
            build_artifact_dir: PathBuf::from(format!(
                "{project_root}/_deps/{library_name}-build"
            )),
            cmake_project_build_root: PathBuf::from(project_root),
        }
    }

    #[test]
    fn empty_registry_returns_none() {
        let reg = BuildAttributionRegistry::from_observations(vec![]);
        assert!(reg.is_empty());
        assert!(reg.lookup("zlib", Path::new("/tmp/bin")).is_none());
    }

    #[test]
    fn single_observation_hit_when_binary_under_project_root() {
        let reg = BuildAttributionRegistry::from_observations(vec![obs(
            "zlib",
            "/tmp/proj/build",
        )]);
        let hit = reg.lookup("zlib", Path::new("/tmp/proj/build/bin/crc-demo"));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().library_name, "zlib");
    }

    #[test]
    fn library_name_match_is_case_insensitive() {
        let reg = BuildAttributionRegistry::from_observations(vec![obs(
            "ZLib",
            "/tmp/proj/build",
        )]);
        // Lowercased lookup against PascalCased declaration name.
        assert!(reg
            .lookup("zlib", Path::new("/tmp/proj/build/bin/x"))
            .is_some());
        // Lookup with the original casing also hits.
        assert!(reg
            .lookup("ZLib", Path::new("/tmp/proj/build/bin/x"))
            .is_some());
        // Different library name doesn't.
        assert!(reg
            .lookup("openssl", Path::new("/tmp/proj/build/bin/x"))
            .is_none());
    }

    #[test]
    fn lookup_returns_none_when_binary_outside_project_scope() {
        let reg = BuildAttributionRegistry::from_observations(vec![obs(
            "zlib",
            "/tmp/proj-a/build",
        )]);
        // Binary lives under project B, not project A.
        let hit = reg.lookup("zlib", Path::new("/tmp/proj-b/build/bin/foo"));
        assert!(hit.is_none());
    }

    #[test]
    fn multi_project_workspace_picks_closest_ancestor() {
        // Outer cmake project + inner cmake project; binary lives
        // under the inner. The inner observation must win.
        let outer = obs("zlib", "/tmp/workspace/build");
        let mut inner = obs("zlib", "/tmp/workspace/subprojects/A/build");
        inner.source_tier_purl = "pkg:github/example/zlib@v2.0".to_string();
        let reg = BuildAttributionRegistry::from_observations(vec![outer, inner]);
        let hit = reg
            .lookup(
                "zlib",
                Path::new("/tmp/workspace/subprojects/A/build/bin/crc"),
            )
            .unwrap();
        assert_eq!(hit.source_tier_purl, "pkg:github/example/zlib@v2.0");
        assert_eq!(
            hit.cmake_project_build_root,
            PathBuf::from("/tmp/workspace/subprojects/A/build")
        );
    }
}
