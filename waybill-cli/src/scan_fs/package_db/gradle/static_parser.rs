//! Milestone 235 US3 — Gradle static parser.
//!
//! MVP scope (m235 Phase 3): only the `extract_direct_coords`
//! helper (T007) that US2 uses to seed its declared-coords list.
//! Full US3 static parser (regex table for 7 patterns × 10
//! configurations × 2 DSLs) lands in a follow-on milestone.
//!
//! See contracts/gradle-static-parser.md for the full contract.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// Lightweight direct-dep coordinate representation used by the
/// (future) US2 cache reader as a seed set. Kept intentionally
/// minimal — the full `MavenCoord` type will land alongside the
/// US2 implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct DirectCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Extract direct-dep coordinates from `build.gradle(.kts)` in the
/// given project directory.
///
/// Covers ONLY the direct-string-coord patterns (skips version catalog
/// references, platform BOMs, project refs). Enough to seed US2's cache
/// lookup; the full US3 parser handles the rest.
///
/// Returns an empty Vec if no `build.gradle(.kts)` file exists.
#[allow(dead_code)] // MVP: not called until US2 lands
pub(super) fn extract_direct_coords(project_dir: &Path) -> Vec<DirectCoord> {
    let mut out: Vec<DirectCoord> = Vec::new();
    for name in ["build.gradle", "build.gradle.kts"] {
        let path = project_dir.join(name);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for cap in groovy_string_coord_re().captures_iter(&content) {
            if let Some(coord) = parse_coord_str(&cap[1]) {
                out.push(coord);
            }
        }
        for cap in kotlin_string_coord_re().captures_iter(&content) {
            if let Some(coord) = parse_coord_str(&cap[1]) {
                out.push(coord);
            }
        }
    }
    out
}

fn parse_coord_str(s: &str) -> Option<DirectCoord> {
    let mut parts = s.splitn(3, ':');
    let group = parts.next()?.trim().to_string();
    let artifact = parts.next()?.trim().to_string();
    let version = parts.next()?.trim().to_string();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some(DirectCoord { group, artifact, version })
}

// Matches Groovy: `implementation 'g:a:v'` or `implementation "g:a:v"`.
// Also matches `api`, `runtimeOnly`, `testImplementation`, etc.
fn groovy_string_coord_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(?:implementation|api|runtimeOnly|compileOnly|testImplementation|testRuntimeOnly|testCompileOnly|annotationProcessor|kapt|ksp)\s+['"]([^'"]+)['"]"#).expect("valid regex")
    })
}

// Matches Kotlin: `implementation("g:a:v")`.
fn kotlin_string_coord_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(?:implementation|api|runtimeOnly|compileOnly|testImplementation|testRuntimeOnly|testCompileOnly|annotationProcessor|kapt|ksp)\s*\(\s*"([^"]+)"\s*\)"#).expect("valid regex")
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn extract_groovy_direct_string_coord() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle",
            r#"
dependencies {
    implementation 'com.example:foo:1.0.0'
    api "com.example:bar:2.0.0"
    testImplementation 'com.example:baz:3.0.0'
}
"#,
        );
        let coords = extract_direct_coords(td.path());
        assert_eq!(coords.len(), 3);
        assert!(coords.iter().any(|c| c.artifact == "foo" && c.version == "1.0.0"));
        assert!(coords.iter().any(|c| c.artifact == "bar" && c.version == "2.0.0"));
        assert!(coords.iter().any(|c| c.artifact == "baz" && c.version == "3.0.0"));
    }

    #[test]
    fn extract_kotlin_direct_string_coord() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle.kts",
            r#"
dependencies {
    implementation("com.example:foo:1.0.0")
    testImplementation("com.example:bar:2.0.0")
}
"#,
        );
        let coords = extract_direct_coords(td.path());
        assert_eq!(coords.len(), 2);
        assert!(coords.iter().any(|c| c.artifact == "foo"));
        assert!(coords.iter().any(|c| c.artifact == "bar"));
    }

    #[test]
    fn empty_when_no_build_files() {
        let td = TempDir::new().unwrap();
        assert!(extract_direct_coords(td.path()).is_empty());
    }
}
