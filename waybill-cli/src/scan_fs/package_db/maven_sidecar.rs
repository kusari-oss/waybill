//! Sidecar POM readers — Fedora/RHEL and Debian layouts.
//!
//! When a JAR's in-archive `META-INF/maven/` metadata is absent
//! (a common build-pipeline outcome on distros that strip it during
//! packaging), waybill recovers the Maven coordinates from the
//! distro's external sidecar POM layout:
//!
//! - **Fedora/RHEL**: `/usr/share/maven-poms/<basename>.pom` — flat
//!   directory, basename-keyed (handled by [`FedoraSidecarIndex`]).
//!   `javapackages-tools` / `xmvn` write effective POMs here during
//!   RPM build.
//! - **Debian/Ubuntu**: `/usr/share/maven-repo/<group-path>/<artifact>/<version>/<artifact>-<version>.pom`
//!   — GAV-tree layout (handled by [`DebianSidecarIndex`]).
//!   `maven-repo-helper` writes per-package POMs here during
//!   `lib*-java` deb-package install.
//!
//! Both layouts produce a basename-keyed index for the same
//! consumer-side lookup pattern in `package_db::maven`.
//!
//! Alpine equivalents remain deferred — Alpine doesn't ship a
//! documented system-wide maven repo convention.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::maven::parse_pom_xml;

/// Common shape across the per-distro sidecar indexes — exposes
/// only the operation that [`resolve_coords`] needs from the
/// caller-supplied index (parent-POM lookup by artifactId during
/// inheritance fallback). Both [`FedoraSidecarIndex`] and
/// [`DebianSidecarIndex`] implement it.
pub(crate) trait SidecarIndex {
    fn lookup_by_artifact_id(&self, artifact_id: &str) -> Option<&Path>;
}

/// Per-scan in-memory index of a rootfs's `/usr/share/maven-poms/`
/// directory. Keyed by the canonical basename (JPP- prefix stripped,
/// `.pom` suffix stripped, ASCII-lowercased).
#[derive(Debug, Default)]
pub(crate) struct FedoraSidecarIndex {
    by_basename: HashMap<String, PathBuf>,
}

impl FedoraSidecarIndex {
    /// Walk `<rootfs>/usr/share/maven-poms/` once and build the
    /// basename → absolute-path index. When a basename is available
    /// under both `JPP-<name>.pom` and plain `<name>.pom`, the
    /// non-prefixed form wins (newer Fedora convention).
    pub(crate) fn build(rootfs: &Path) -> Self {
        let dir = rootfs.join("usr/share/maven-poms");
        let mut by_basename: HashMap<String, PathBuf> = HashMap::new();
        // Two-pass walk: first record `JPP-*.pom`, then let plain
        // `<name>.pom` overwrite on basename collision.
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => return Self { by_basename },
        };
        let mut plain: Vec<PathBuf> = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !fname.ends_with(".pom") {
                continue;
            }
            let stem = &fname[..fname.len() - 4];
            if let Some(rest) = stem.strip_prefix("JPP-") {
                by_basename.insert(rest.to_ascii_lowercase(), path);
            } else {
                plain.push(path);
            }
        }
        for path in plain {
            let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let stem = &fname[..fname.len().saturating_sub(4)];
            by_basename.insert(stem.to_ascii_lowercase(), path);
        }
        Self { by_basename }
    }

    /// Look up a JAR by filename basename. Strips any trailing
    /// `-<version>` segment from the JAR filename before matching —
    /// Fedora sidecar POMs are version-agnostic because each RPM
    /// installs exactly one version. `guice-5.1.0.jar` → key `"guice"`.
    pub(crate) fn lookup_for_jar(&self, jar_path: &Path) -> Option<&Path> {
        let fname = jar_path.file_name().and_then(|s| s.to_str())?;
        if !fname.ends_with(".jar") {
            return None;
        }
        let stem = &fname[..fname.len() - 4];
        let basename = strip_trailing_version(stem).to_ascii_lowercase();
        self.by_basename.get(basename.as_str()).map(|p| p.as_path())
    }

    /// `true` when the index contains zero entries — used by callers
    /// to skip sidecar resolution entirely when the rootfs isn't
    /// Fedora-shaped.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_basename.is_empty()
    }

    /// Number of indexed sidecar POMs. Used for scan-summary logging.
    pub(crate) fn len(&self) -> usize {
        self.by_basename.len()
    }

    /// Look up a parent POM by its artifactId only — Fedora's flat
    /// layout keys sidecars by artifact, not by full GAV. Called
    /// during one-level parent inheritance resolution.
    pub(crate) fn lookup_by_artifact_id(&self, artifact_id: &str) -> Option<&Path> {
        self.by_basename
            .get(artifact_id.to_ascii_lowercase().as_str())
            .map(|p| p.as_path())
    }
}

impl SidecarIndex for FedoraSidecarIndex {
    fn lookup_by_artifact_id(&self, artifact_id: &str) -> Option<&Path> {
        FedoraSidecarIndex::lookup_by_artifact_id(self, artifact_id)
    }
}

/// Per-scan in-memory index of a rootfs's `/usr/share/maven-repo/`
/// GAV directory tree. Same `by_basename` shape as
/// [`FedoraSidecarIndex`] so consumer-side lookups don't need to
/// know which distro produced the POM.
///
/// The Debian Java packaging policy (see Debian's `maven-repo-helper`)
/// places `<artifact>-<version>.pom` files at
/// `/usr/share/maven-repo/<group-with-slashes>/<artifact>/<version>/`.
/// We walk the tree and key by `<artifact>` (lowercased), matching
/// the lookup-by-jar-basename pattern that already drives the
/// Fedora sidecar.
#[derive(Debug, Default)]
pub(crate) struct DebianSidecarIndex {
    by_basename: HashMap<String, PathBuf>,
}

impl DebianSidecarIndex {
    /// Walk `<rootfs>/usr/share/maven-repo/` recursively and
    /// build the basename → POM-path index. Recursion is depth-
    /// capped to avoid getting stuck in pathological symlink
    /// cycles; a depth of 8 segments comfortably exceeds any
    /// realistic GAV depth (groups rarely exceed 5–6 segments).
    pub(crate) fn build(rootfs: &Path) -> Self {
        let mut by_basename: HashMap<String, PathBuf> = HashMap::new();
        let root = rootfs.join("usr/share/maven-repo");
        if !root.exists() {
            return Self { by_basename };
        }
        Self::walk(&root, 0, &mut by_basename);
        Self { by_basename }
    }

    // SAFETY (milestone-054 walker audit): symlink-loop protection
    // comes from `entry.file_type()` (lstat-equivalent — does NOT
    // dereference symlinks; see line 171 below). Plus a depth-cap
    // backstop at 8 (`MAX_DEPTH`). Per FR-001 audit rubric option
    // (b): the lstat skip is the primary invariant, depth cap is
    // defense-in-depth.
    /// Recursive walker. Caps depth at 8 to bound work in
    /// pathological cases (cycles, intentional deep nesting).
    fn walk(dir: &Path, depth: usize, out: &mut HashMap<String, PathBuf>) {
        const MAX_DEPTH: usize = 8;
        if depth > MAX_DEPTH {
            return;
        }
        let read = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return,
        };
        for entry in read.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                Self::walk(&path, depth + 1, out);
                continue;
            }
            // Leaf POM: `<artifact>-<version>.pom`. Strip the
            // `.pom` suffix and the trailing `-<version>` segment to
            // recover the artifact-name basename used as the key
            // (matches the Fedora index's basename-key convention).
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !fname.ends_with(".pom") {
                continue;
            }
            let stem = &fname[..fname.len() - 4];
            let basename = strip_trailing_version(stem).to_ascii_lowercase();
            if basename.is_empty() {
                continue;
            }
            // First-write-wins: if the GAV tree happens to have two
            // POMs reducing to the same basename (extremely rare —
            // would mean two different groups shipping the same
            // artifact-name), the first one wins. The lookup-by-
            // basename API can't disambiguate these anyway.
            out.entry(basename).or_insert(path);
        }
    }

    /// Same lookup contract as [`FedoraSidecarIndex::lookup_for_jar`]:
    /// strip the trailing `-<version>` segment from the JAR
    /// filename, match the lowercased basename against the index.
    pub(crate) fn lookup_for_jar(&self, jar_path: &Path) -> Option<&Path> {
        let fname = jar_path.file_name().and_then(|s| s.to_str())?;
        if !fname.ends_with(".jar") {
            return None;
        }
        let stem = &fname[..fname.len() - 4];
        let basename = strip_trailing_version(stem).to_ascii_lowercase();
        self.by_basename.get(basename.as_str()).map(|p| p.as_path())
    }

    /// `true` when the index contains zero entries — used by
    /// callers to skip Debian-sidecar resolution entirely when the
    /// rootfs isn't Debian-shaped.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_basename.is_empty()
    }

    /// Number of indexed sidecar POMs. Mirrors the Fedora variant.
    pub(crate) fn len(&self) -> usize {
        self.by_basename.len()
    }

    /// Look up a parent POM by its artifactId for one-level
    /// inheritance resolution. Same shape as
    /// [`FedoraSidecarIndex::lookup_by_artifact_id`].
    pub(crate) fn lookup_by_artifact_id(&self, artifact_id: &str) -> Option<&Path> {
        self.by_basename
            .get(artifact_id.to_ascii_lowercase().as_str())
            .map(|p| p.as_path())
    }
}

impl SidecarIndex for DebianSidecarIndex {
    fn lookup_by_artifact_id(&self, artifact_id: &str) -> Option<&Path> {
        DebianSidecarIndex::lookup_by_artifact_id(self, artifact_id)
    }
}

/// Strip a trailing `-<digits-and-dots-and-letters>` version component
/// from a JAR basename. The split point is the last `-` followed by a
/// digit. Non-versioned names (e.g. `aopalliance` with no trailing
/// version) fall through unchanged.
fn strip_trailing_version(stem: &str) -> &str {
    let bytes = stem.as_bytes();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'-' {
            if let Some(next) = bytes.get(i + 1) {
                if next.is_ascii_digit() {
                    return &stem[..i];
                }
            }
        }
    }
    stem
}

/// Resolved coordinates from a sidecar POM, with one-level parent
/// inheritance applied when the parent POM is present in the same
/// index. Returns `None` when a complete `(groupId, artifactId,
/// version)` triple cannot be assembled.
///
/// Generic over [`SidecarIndex`] so callers can pass either a
/// [`FedoraSidecarIndex`] or a [`DebianSidecarIndex`] (milestone 042
/// US2). The parent-inheritance lookup is the only operation
/// resolve_coords actually performs against the index — both
/// implementations key by lowercase artifact-id.
pub(crate) fn resolve_coords(
    sidecar_path: &Path,
    index: &dyn SidecarIndex,
) -> Option<(String, String, String)> {
    let bytes = std::fs::read(sidecar_path).ok()?;
    let doc = parse_pom_xml(&bytes);
    // Seed from self_coord when fully present, otherwise split into
    // separate channels (self_artifact_id is always set when an
    // <artifactId> appears on the project element, even if groupId /
    // version are absent and inherited from <parent>).
    let (mut g, a, mut v): (Option<String>, Option<String>, Option<String>) =
        match doc.self_coord.clone() {
            Some((g, a, v)) => (Some(g), Some(a), Some(v)),
            None => (None, doc.self_artifact_id.clone(), None),
        };
    // Fedora child POMs typically omit `<groupId>` and `<version>`,
    // inheriting both from `<parent>`. Apply one level of inheritance.
    if let Some((pg, _pa, pv)) = &doc.parent_coord {
        if g.as_deref().unwrap_or("").is_empty() {
            g = Some(pg.clone());
        }
        if v.as_deref().unwrap_or("").is_empty() {
            v = Some(pv.clone());
        }
    }
    // When we still don't have groupId or version and the parent POM
    // is on disk in the same index, consult it for its own self-coord
    // as a secondary inheritance source.
    if g.as_deref().unwrap_or("").is_empty() || v.as_deref().unwrap_or("").is_empty() {
        if let Some((_pg, pa, _pv)) = &doc.parent_coord {
            if let Some(parent_path) = index.lookup_by_artifact_id(pa) {
                if let Ok(parent_bytes) = std::fs::read(parent_path) {
                    let parent_doc = parse_pom_xml(&parent_bytes);
                    if let Some((pg2, _pa2, pv2)) = &parent_doc.self_coord {
                        if g.as_deref().unwrap_or("").is_empty() {
                            g = Some(pg2.clone());
                        }
                        if v.as_deref().unwrap_or("").is_empty() {
                            v = Some(pv2.clone());
                        }
                    }
                }
            }
        }
    }
    match (g, a, v) {
        (Some(g), Some(a), Some(v))
            if !g.is_empty() && !a.is_empty() && !v.is_empty() =>
        {
            Some((g, a, v))
        }
        _ => None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn pom_text(group: &str, artifact: &str, version: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
               <modelVersion>4.0.0</modelVersion>\n\
               <groupId>{group}</groupId>\n\
               <artifactId>{artifact}</artifactId>\n\
               <version>{version}</version>\n\
             </project>\n"
        )
    }

    #[test]
    fn strips_trailing_version_basic() {
        assert_eq!(strip_trailing_version("guice-5.1.0"), "guice");
        assert_eq!(strip_trailing_version("aopalliance-1.0"), "aopalliance");
        assert_eq!(
            strip_trailing_version("commons-compress-1.21"),
            "commons-compress"
        );
    }

    #[test]
    fn strips_trailing_version_leaves_non_versioned_alone() {
        assert_eq!(strip_trailing_version("aopalliance"), "aopalliance");
        assert_eq!(strip_trailing_version("foo-bar"), "foo-bar");
    }

    #[test]
    fn index_empty_when_dir_missing() {
        let tmp = tempdir().unwrap();
        let idx = FedoraSidecarIndex::build(tmp.path());
        assert!(idx.is_empty());
    }

    #[test]
    fn index_picks_up_jpp_prefixed_and_plain() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        write(&dir.join("JPP-guice.pom"), &pom_text("g", "guice", "1"));
        write(&dir.join("aopalliance.pom"), &pom_text("g", "a", "1"));
        let idx = FedoraSidecarIndex::build(tmp.path());
        assert!(idx.lookup_by_artifact_id("guice").is_some());
        assert!(idx.lookup_by_artifact_id("aopalliance").is_some());
    }

    #[test]
    fn plain_name_wins_on_basename_collision() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        write(&dir.join("JPP-dup.pom"), &pom_text("g1", "dup", "1"));
        write(&dir.join("dup.pom"), &pom_text("g2", "dup", "2"));
        let idx = FedoraSidecarIndex::build(tmp.path());
        let hit = idx.lookup_by_artifact_id("dup").unwrap();
        // The non-JPP file wins — confirm by filename.
        assert_eq!(
            hit.file_name().and_then(|s| s.to_str()).unwrap(),
            "dup.pom"
        );
    }

    #[test]
    fn lookup_for_jar_strips_version_suffix() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        write(&dir.join("JPP-guice.pom"), &pom_text("com.google.inject", "guice", "5.1.0"));
        let idx = FedoraSidecarIndex::build(tmp.path());
        let jar = tmp
            .path()
            .join("usr/share/maven/lib/guice-5.1.0.jar");
        assert!(idx.lookup_for_jar(&jar).is_some());
    }

    #[test]
    fn lookup_for_jar_miss_returns_none() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        write(&dir.join("JPP-guice.pom"), &pom_text("g", "guice", "5.1.0"));
        let idx = FedoraSidecarIndex::build(tmp.path());
        let jar = tmp
            .path()
            .join("usr/share/maven/lib/logback-1.2.jar");
        assert!(idx.lookup_for_jar(&jar).is_none());
    }

    #[test]
    fn resolve_coords_direct_self_coord() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        let pom_path = dir.join("JPP-guice.pom");
        write(
            &pom_path,
            &pom_text("com.google.inject", "guice", "5.1.0"),
        );
        let idx = FedoraSidecarIndex::build(tmp.path());
        let coords = resolve_coords(&pom_path, &idx).unwrap();
        assert_eq!(
            coords,
            (
                "com.google.inject".to_string(),
                "guice".to_string(),
                "5.1.0".to_string()
            )
        );
    }

    #[test]
    fn resolve_coords_inherits_groupid_from_parent() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        let parent_path = dir.join("guice-parent.pom");
        let child_path = dir.join("guice-child.pom");
        write(
            &parent_path,
            &pom_text("com.google.inject", "guice-parent", "5.1.0"),
        );
        write(
            &child_path,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
               <modelVersion>4.0.0</modelVersion>\n\
               <parent>\n\
                 <groupId>com.google.inject</groupId>\n\
                 <artifactId>guice-parent</artifactId>\n\
                 <version>5.1.0</version>\n\
               </parent>\n\
               <artifactId>guice-child</artifactId>\n\
             </project>\n",
        );
        let idx = FedoraSidecarIndex::build(tmp.path());
        let coords = resolve_coords(&child_path, &idx).unwrap();
        assert_eq!(
            coords,
            (
                "com.google.inject".to_string(),
                "guice-child".to_string(),
                "5.1.0".to_string()
            )
        );
    }

    #[test]
    fn resolve_coords_returns_none_on_incomplete_child_without_parent() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("usr/share/maven-poms");
        let child_path = dir.join("orphan.pom");
        write(
            &child_path,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
               <modelVersion>4.0.0</modelVersion>\n\
               <artifactId>orphan</artifactId>\n\
             </project>\n",
        );
        let idx = FedoraSidecarIndex::build(tmp.path());
        assert!(resolve_coords(&child_path, &idx).is_none());
    }

    // ---- Milestone 042 US2: DebianSidecarIndex --------------------------

    /// Helper: write a Debian-shaped sidecar POM under
    /// `<rootfs>/usr/share/maven-repo/<group-path>/<artifact>/<version>/<artifact>-<version>.pom`.
    fn write_debian_pom(
        rootfs: &Path,
        group: &str,
        artifact: &str,
        version: &str,
    ) -> PathBuf {
        let group_path: PathBuf = group.split('.').collect();
        let dir = rootfs
            .join("usr/share/maven-repo")
            .join(&group_path)
            .join(artifact)
            .join(version);
        std::fs::create_dir_all(&dir).unwrap();
        let pom_path = dir.join(format!("{artifact}-{version}.pom"));
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
               <modelVersion>4.0.0</modelVersion>\n\
               <groupId>{group}</groupId>\n\
               <artifactId>{artifact}</artifactId>\n\
               <version>{version}</version>\n\
             </project>\n",
        );
        std::fs::write(&pom_path, body).unwrap();
        pom_path
    }

    #[test]
    fn debian_sidecar_index_extracts_canonical_gav() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = write_debian_pom(
            tmp.path(),
            "org.apache.commons",
            "commons-lang3",
            "3.12.0",
        );
        let idx = DebianSidecarIndex::build(tmp.path());
        assert!(!idx.is_empty());
        assert_eq!(idx.len(), 1);
        // Lookup by JAR basename should find the POM.
        let jar = tmp.path().join("usr/share/java/commons-lang3-3.12.0.jar");
        let resolved = idx.lookup_for_jar(&jar).expect("lookup should resolve");
        assert_eq!(resolved, pom_path);
        // resolve_coords through the trait yields the right GAV.
        let (g, a, v) = resolve_coords(&pom_path, &idx).expect("resolve_coords");
        assert_eq!(g, "org.apache.commons");
        assert_eq!(a, "commons-lang3");
        assert_eq!(v, "3.12.0");
    }

    #[test]
    fn debian_sidecar_index_handles_multi_segment_groups() {
        let tmp = tempfile::tempdir().unwrap();
        write_debian_pom(
            tmp.path(),
            "org.apache.maven.plugins",
            "maven-compiler-plugin",
            "3.11.0",
        );
        let idx = DebianSidecarIndex::build(tmp.path());
        let jar = tmp.path().join("usr/share/java/maven-compiler-plugin-3.11.0.jar");
        let pom = idx.lookup_for_jar(&jar).expect("multi-segment group should resolve");
        let (g, a, v) = resolve_coords(pom, &idx).expect("resolve_coords");
        assert_eq!(g, "org.apache.maven.plugins");
        assert_eq!(a, "maven-compiler-plugin");
        assert_eq!(v, "3.11.0");
    }

    #[test]
    fn debian_sidecar_index_handles_version_with_build_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        write_debian_pom(tmp.path(), "com.example", "foo", "1.0.0-SNAPSHOT");
        let idx = DebianSidecarIndex::build(tmp.path());
        let jar = tmp.path().join("usr/share/java/foo-1.0.0-SNAPSHOT.jar");
        let pom = idx.lookup_for_jar(&jar).expect("snapshot version should resolve");
        let (_, a, v) = resolve_coords(pom, &idx).expect("resolve_coords");
        assert_eq!(a, "foo");
        assert_eq!(v, "1.0.0-SNAPSHOT");
    }

    #[test]
    fn debian_sidecar_index_returns_empty_for_missing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        // No /usr/share/maven-repo/ at all.
        let idx = DebianSidecarIndex::build(tmp.path());
        assert!(idx.is_empty());
    }

    #[test]
    fn debian_sidecar_index_returns_empty_for_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("usr/share/maven-repo")).unwrap();
        let idx = DebianSidecarIndex::build(tmp.path());
        assert!(idx.is_empty());
    }

    /// FR-005: the Fedora and Debian indexes are independent
    /// surfaces; each is tried in order at the call site. This
    /// test verifies both indexes resolve their own JARs cleanly
    /// when both layouts coexist.
    #[test]
    fn fedora_and_debian_indexes_coexist_independently() {
        let tmp = tempfile::tempdir().unwrap();
        // Fedora layout: flat /usr/share/maven-poms/.
        let fedora_dir = tmp.path().join("usr/share/maven-poms");
        std::fs::create_dir_all(&fedora_dir).unwrap();
        std::fs::write(
            fedora_dir.join("guice.pom"),
            "<?xml version=\"1.0\"?>\n\
             <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
               <modelVersion>4.0.0</modelVersion>\n\
               <groupId>com.google.inject</groupId>\n\
               <artifactId>guice</artifactId>\n\
               <version>5.1.0</version>\n\
             </project>\n",
        )
        .unwrap();
        // Debian layout: GAV tree.
        write_debian_pom(
            tmp.path(),
            "org.apache.commons",
            "commons-lang3",
            "3.12.0",
        );

        let fedora_idx = FedoraSidecarIndex::build(tmp.path());
        let debian_idx = DebianSidecarIndex::build(tmp.path());
        assert_eq!(fedora_idx.len(), 1);
        assert_eq!(debian_idx.len(), 1);

        // Fedora-keyed JAR resolves only via Fedora index.
        let guice_jar = tmp.path().join("usr/share/java/guice-5.1.0.jar");
        assert!(fedora_idx.lookup_for_jar(&guice_jar).is_some());
        assert!(
            debian_idx.lookup_for_jar(&guice_jar).is_none(),
            "Fedora-only JAR should not resolve via Debian index"
        );

        // Debian-keyed JAR resolves only via Debian index.
        let lang3_jar = tmp
            .path()
            .join("usr/share/java/commons-lang3-3.12.0.jar");
        assert!(debian_idx.lookup_for_jar(&lang3_jar).is_some());
        assert!(
            fedora_idx.lookup_for_jar(&lang3_jar).is_none(),
            "Debian-only JAR should not resolve via Fedora index"
        );
    }
}
