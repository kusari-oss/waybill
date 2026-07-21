//! Parser for NuGet Central Package Management (`Directory.Packages.props`)
//! (milestone 106 US4, FR-007a).
//!
//! When a project uses CPM, individual `.csproj` files declare package
//! references WITHOUT a `Version=` attribute; the version comes from a
//! `<PackageVersion Include="X" Version="..."/>` entry in a
//! `Directory.Packages.props` file in some ancestor directory.
//!
//! This module:
//! - Parses the props file into an `Include`-keyed lookup map.
//! - Walks up from a `.csproj`'s directory to find the closest props.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

/// Maps package `Include=` (case-preserved) to the version string from
/// the props file.
pub(super) type CpmMap = HashMap<String, String>;

/// Parse a `Directory.Packages.props` file. Returns an empty map on read
/// or parse failure (warns via `tracing::warn!`).
pub(super) fn parse_props(path: &Path) -> CpmMap {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read Directory.Packages.props (skipping; FR-015)"
            );
            return CpmMap::new();
        }
    };
    parse_bytes(&bytes, path)
}

fn parse_bytes(bytes: &[u8], path: &Path) -> CpmMap {
    let mut reader = Reader::from_reader(bytes);
    reader.trim_text(true);

    let mut map = CpmMap::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                if !is_package_version(e.name().as_ref()) {
                    continue;
                }
                let mut include: Option<String> = None;
                let mut version: Option<String> = None;
                for attr in e.attributes().with_checks(false).flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let val = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                    match key.as_str() {
                        "include" => include = Some(val),
                        "version" => version = Some(val),
                        _ => {}
                    }
                }
                if let (Some(i), Some(v)) = (include, version) {
                    if !i.is_empty() && !v.is_empty() {
                        map.insert(i, v);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to parse Directory.Packages.props (returning partial map; FR-015)"
                );
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    map
}

fn is_package_version(name: &[u8]) -> bool {
    let s = std::str::from_utf8(name).unwrap_or("");
    let local = s.rsplit(':').next().unwrap_or(s);
    local.eq_ignore_ascii_case("PackageVersion")
}

/// Walk up from `start_dir` looking for `Directory.Packages.props`.
/// Returns the path of the CLOSEST props file (MSBuild semantics).
/// Returns `None` if no props file is found before reaching either
/// `scan_root` or the filesystem root.
///
/// `scan_root` bounds the search — we never walk outside the scanned
/// rootfs. Both arguments should be absolute paths or both should be
/// under the same logical base; the comparison uses path-prefix.
pub(super) fn find_props_walking_up(
    start_dir: &Path,
    scan_root: &Path,
) -> Option<PathBuf> {
    let mut cursor: Option<&Path> = Some(start_dir);
    while let Some(dir) = cursor {
        let candidate = dir.join("Directory.Packages.props");
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir == scan_root {
            // Don't walk above the scan root.
            return None;
        }
        cursor = dir.parent();
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_package_version_entries() {
        let xml = r#"<?xml version="1.0"?>
<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.Cpm" Version="9.0.1" />
    <PackageVersion Include="MikebomFixture.OtherCpm" Version="2.0.0" />
  </ItemGroup>
</Project>"#;
        let map = parse_bytes(xml.as_bytes(), Path::new("Directory.Packages.props"));
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("MikebomFixture.Cpm"), Some(&"9.0.1".to_string()));
        assert_eq!(
            map.get("MikebomFixture.OtherCpm"),
            Some(&"2.0.0".to_string())
        );
    }

    #[test]
    fn ignores_unrelated_elements() {
        let xml = r#"<Project>
  <ItemGroup>
    <PackageReference Include="ShouldBeIgnored" Version="0.0.1" />
  </ItemGroup>
</Project>"#;
        let map = parse_bytes(xml.as_bytes(), Path::new("Directory.Packages.props"));
        assert!(map.is_empty());
    }

    #[test]
    // walker-audit: false-positive — #[test] function name shares the walk_up_ prefix of the unit under test
    fn walk_up_finds_props_in_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path();
        let inner = scan_root.join("src").join("MyApp");
        std::fs::create_dir_all(&inner).unwrap();
        let props = scan_root.join("Directory.Packages.props");
        std::fs::write(&props, "<Project/>").unwrap();
        let found = find_props_walking_up(&inner, scan_root);
        assert_eq!(found.as_deref(), Some(props.as_path()));
    }

    #[test]
    // walker-audit: false-positive — #[test] function name shares the walk_up_ prefix of the unit under test
    fn walk_up_stops_at_scan_root() {
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path().join("project");
        let inner = scan_root.join("src");
        std::fs::create_dir_all(&inner).unwrap();
        // Props above scan_root must NOT be found.
        let above = tmp.path().join("Directory.Packages.props");
        std::fs::write(&above, "<Project/>").unwrap();
        let found = find_props_walking_up(&inner, &scan_root);
        assert!(found.is_none(), "must not walk above scan_root; got {found:?}");
    }
}
