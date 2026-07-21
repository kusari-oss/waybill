//! Component-role classifier (milestone 048).
//!
//! Classifies a `ResolvedComponent` into one of three roles based
//! on the filesystem locations recorded in its
//! `evidence.occurrences[]`:
//!
//! - [`ComponentRole::BuildTool`] — components living under
//!   build-tool installation paths (Maven, Gradle, sbt). Annotated
//!   as `mikebom:component-role = "build-tool"`.
//! - [`ComponentRole::LanguageRuntime`] — components living under
//!   language-runtime system-installed paths (JDK, system Python,
//!   system Node). Annotated as
//!   `mikebom:component-role = "language-runtime"`.
//! - **Absent** (no annotation emitted) — when no occurrence
//!   matches any heuristic-table entry. Three-state semantics:
//!   absence does NOT mean "definitely application code"; it
//!   means "this heuristic didn't classify".
//!
//! The classifier runs late in the resolve pipeline (after dedup,
//! when `evidence.occurrences[]` is fully populated). The
//! resulting role goes into the component's
//! `extra_annotations` bag, which the established
//! generic-annotation flow surfaces in CDX `properties[]`,
//! SPDX 2.3 `packages[].annotations[]`, and SPDX 3 top-level
//! `annotations[]` via the parity-extractors framework.
//!
//! The heuristic table is curated and small — extension is a
//! mechanical follow-on as new build-tool / language-runtime
//! installation paths surface from real images.

use waybill_common::resolution::FileOccurrence;

/// Filesystem-position-determined component role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    /// Build-tooling component (Maven, Gradle, sbt internals).
    BuildTool,
    /// Language-runtime component (JDK, system Python, system
    /// Node — platform-managed runtime libraries, not
    /// application code).
    LanguageRuntime,
}

impl ComponentRole {
    /// User-facing string value emitted in the
    /// `mikebom:component-role` annotation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentRole::BuildTool => "build-tool",
            ComponentRole::LanguageRuntime => "language-runtime",
        }
    }
}

/// Curated path-prefix heuristic table. Each entry is a
/// `(pattern, role)` pair; the first occurrence whose `location`
/// matches any pattern determines the component's role.
///
/// Patterns support a single-segment glob via `*` characters
/// inside a path segment. The pattern's literal segments must
/// match exactly; segments containing `*` are treated as
/// single-segment globs (`python*` matches `python3.11`,
/// `*-debian` matches `foo-debian`, bare `*` matches anything).
///
/// A trailing `/` (path-prefix mode) lets paths under the
/// pattern match: `/usr/share/maven/lib/sub/foo.jar` matches
/// pattern `/usr/share/maven/lib/`.
const HEURISTIC_TABLE: &[(&str, ComponentRole)] = &[
    // Build tools.
    ("/usr/share/maven/lib/", ComponentRole::BuildTool),
    ("/usr/share/gradle/lib/", ComponentRole::BuildTool),
    ("/opt/sbt/", ComponentRole::BuildTool),
    // Language runtimes.
    ("/usr/lib/jvm/*/lib/", ComponentRole::LanguageRuntime),
    ("/usr/lib/node_modules/", ComponentRole::LanguageRuntime),
    ("/usr/lib/python*/site-packages/", ComponentRole::LanguageRuntime),
    ("/usr/lib/python*/dist-packages/", ComponentRole::LanguageRuntime),
];

/// Classify a component's role based on its
/// `evidence.occurrences[]` paths. Returns the role of the FIRST
/// occurrence that matches any heuristic-table entry, in the
/// natural order of `occurrences`. Returns `None` when no
/// heuristic hits.
pub fn classify(occurrences: &[FileOccurrence]) -> Option<ComponentRole> {
    for occ in occurrences {
        for (pattern, role) in HEURISTIC_TABLE {
            if matches_pattern(pattern, &occ.location) {
                return Some(*role);
            }
        }
    }
    None
}

/// Single-segment-glob path matcher.
///
/// Splits both `pattern` and `path` on `/` and walks segments in
/// parallel. A literal pattern segment must equal the path
/// segment; a pattern segment containing `*` is a single-segment
/// glob (matches any one path segment whose literal prefix and
/// suffix surrounding the `*` are present).
///
/// When the pattern has a trailing empty segment (i.e., ends with
/// `/`), the matcher allows arbitrary trailing path segments —
/// `/usr/share/maven/lib/foo/bar.jar` matches pattern
/// `/usr/share/maven/lib/`. When the pattern has no trailing
/// `/`, segment counts must match exactly.
fn matches_pattern(pattern: &str, path: &str) -> bool {
    let pat_segments: Vec<&str> = pattern.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();

    // Trailing-slash detection: `/usr/share/maven/lib/` splits to
    // `["", "usr", "share", "maven", "lib", ""]` — the last
    // element is empty when the pattern ends with `/`.
    let prefix_mode = pat_segments
        .last()
        .map(|s| s.is_empty())
        .unwrap_or(false);
    let pat_significant: &[&str] = if prefix_mode {
        &pat_segments[..pat_segments.len() - 1]
    } else {
        &pat_segments[..]
    };

    if prefix_mode {
        // Path must have STRICTLY MORE segments than the
        // significant pattern — there must be something UNDER the
        // prefix directory. Pattern `/usr/share/maven/lib/` should
        // match `/usr/share/maven/lib/foo.jar` but NOT the bare
        // `/usr/share/maven/lib` directory entry itself.
        if path_segments.len() <= pat_significant.len() {
            return false;
        }
    } else {
        // Path must have EXACTLY as many segments as the pattern.
        if path_segments.len() != pat_significant.len() {
            return false;
        }
    }

    for (i, pat_seg) in pat_significant.iter().enumerate() {
        let path_seg = path_segments[i];
        if !matches_segment(pat_seg, path_seg) {
            return false;
        }
    }
    true
}

/// Match a single path segment against a single pattern segment.
/// `*` in the pattern is a single-segment glob: it matches any
/// content in this segment, with required literal prefix/suffix
/// surrounding it.
fn matches_segment(pat: &str, path: &str) -> bool {
    if !pat.contains('*') {
        return pat == path;
    }
    // Split on `*` once; pattern segments only need single-`*`
    // support for the curated cases we care about (`python*`,
    // `*-debian`, bare `*`).
    if let Some(star_idx) = pat.find('*') {
        let prefix = &pat[..star_idx];
        let suffix = &pat[star_idx + 1..];
        return path.starts_with(prefix)
            && path.ends_with(suffix)
            // Avoid double-counting when prefix + suffix overlap on
            // a too-short path (e.g., pattern `foo*bar` against path
            // `foob` shouldn't match).
            && path.len() >= prefix.len() + suffix.len();
    }
    pat == path
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn occ(location: &str) -> FileOccurrence {
        FileOccurrence {
            location: location.to_string(),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            md5_legacy: None,
            apk_sha1: None,
            rpm_file_digest: None,
        }
    }

    #[test]
    fn matches_pattern_literal_prefix() {
        assert!(matches_pattern(
            "/usr/share/maven/lib/",
            "/usr/share/maven/lib/maven-artifact-3.1.0.jar"
        ));
        assert!(matches_pattern(
            "/usr/share/maven/lib/",
            "/usr/share/maven/lib/sub/dir/nested.jar"
        ));
        assert!(!matches_pattern(
            "/usr/share/maven/lib/",
            "/usr/share/maven/lib"
        ));
        assert!(!matches_pattern(
            "/usr/share/maven/lib/",
            "/usr/share/maven/foo/bar.jar"
        ));
        assert!(!matches_pattern(
            "/usr/share/maven/lib/",
            "/app/lib/foo.jar"
        ));
    }

    #[test]
    fn matches_pattern_single_segment_glob_jvm() {
        assert!(matches_pattern(
            "/usr/lib/jvm/*/lib/",
            "/usr/lib/jvm/java-21-openjdk/lib/jrt-fs.jar"
        ));
        assert!(matches_pattern(
            "/usr/lib/jvm/*/lib/",
            "/usr/lib/jvm/openjdk-17/lib/something.jar"
        ));
        // `*` matches one segment, not multiple.
        assert!(!matches_pattern(
            "/usr/lib/jvm/*/lib/",
            "/usr/lib/jvm/sub/dir/lib/foo.jar"
        ));
        // Wrong location — not under jvm at all.
        assert!(!matches_pattern(
            "/usr/lib/jvm/*/lib/",
            "/usr/lib/python3/site-packages/foo.py"
        ));
    }

    #[test]
    fn matches_pattern_prefix_glob_python() {
        assert!(matches_pattern(
            "/usr/lib/python*/site-packages/",
            "/usr/lib/python3.11/site-packages/foo/bar.py"
        ));
        assert!(matches_pattern(
            "/usr/lib/python*/dist-packages/",
            "/usr/lib/python3/dist-packages/debian-package/__init__.py"
        ));
        // venv path is NOT a system runtime; should not match.
        assert!(!matches_pattern(
            "/usr/lib/python*/site-packages/",
            "/app/.venv/lib/python3.11/site-packages/foo/bar.py"
        ));
    }

    #[test]
    fn classify_returns_build_tool_for_maven_lib() {
        let occs = vec![occ("/usr/share/maven/lib/maven-artifact-3.1.0.jar")];
        assert_eq!(classify(&occs), Some(ComponentRole::BuildTool));
    }

    #[test]
    fn classify_returns_build_tool_for_gradle_lib() {
        let occs = vec![occ("/usr/share/gradle/lib/gradle-core.jar")];
        assert_eq!(classify(&occs), Some(ComponentRole::BuildTool));
    }

    #[test]
    fn classify_returns_build_tool_for_sbt() {
        let occs = vec![occ("/opt/sbt/bin/sbt-launch.jar")];
        assert_eq!(classify(&occs), Some(ComponentRole::BuildTool));
    }

    #[test]
    fn classify_returns_language_runtime_for_jdk() {
        let occs = vec![occ("/usr/lib/jvm/java-21-openjdk/lib/jrt-fs.jar")];
        assert_eq!(classify(&occs), Some(ComponentRole::LanguageRuntime));
    }

    #[test]
    fn classify_returns_language_runtime_for_system_node() {
        let occs = vec![occ("/usr/lib/node_modules/foo/index.js")];
        assert_eq!(classify(&occs), Some(ComponentRole::LanguageRuntime));
    }

    #[test]
    fn classify_returns_language_runtime_for_system_python_site_packages() {
        let occs = vec![occ("/usr/lib/python3.11/site-packages/foo/__init__.py")];
        assert_eq!(classify(&occs), Some(ComponentRole::LanguageRuntime));
    }

    #[test]
    fn classify_returns_language_runtime_for_system_python_dist_packages() {
        let occs = vec![occ("/usr/lib/python3/dist-packages/debian-pkg/__init__.py")];
        assert_eq!(classify(&occs), Some(ComponentRole::LanguageRuntime));
    }

    #[test]
    fn classify_returns_none_for_application_paths() {
        // Three-state semantics: absence ≠ application; this just
        // means the heuristic didn't classify it.
        let occs = vec![occ("/app/lib/foo.jar")];
        assert_eq!(classify(&occs), None);

        let occs = vec![occ("/opt/myapp/bin/run.sh")];
        assert_eq!(classify(&occs), None);

        let occs = vec![occ("/home/user/code/build.gradle")];
        assert_eq!(classify(&occs), None);
    }

    #[test]
    fn classify_returns_none_for_empty_occurrences() {
        assert_eq!(classify(&[]), None);
    }

    #[test]
    fn classify_returns_first_hit_for_mixed_paths() {
        // Component with multiple occurrences, only some matching.
        // First occurrence in the slice wins per FR semantics.
        let occs = vec![
            occ("/app/lib/foo.jar"),  // No match.
            occ("/usr/share/maven/lib/foo.jar"),  // Build-tool.
        ];
        assert_eq!(classify(&occs), Some(ComponentRole::BuildTool));

        // Order matters — different ordering produces different
        // first-hit (but both hits are valid; this is documented
        // edge-case behavior per spec).
        let occs = vec![
            occ("/usr/lib/jvm/openjdk-21/lib/jrt-fs.jar"),  // Language-runtime.
            occ("/usr/share/maven/lib/foo.jar"),  // Build-tool (also matches).
        ];
        assert_eq!(classify(&occs), Some(ComponentRole::LanguageRuntime));
    }

    #[test]
    fn as_str_returns_correct_annotation_values() {
        assert_eq!(ComponentRole::BuildTool.as_str(), "build-tool");
        assert_eq!(ComponentRole::LanguageRuntime.as_str(), "language-runtime");
    }
}
