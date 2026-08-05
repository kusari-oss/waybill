//! `Directory.Build.props` + `Directory.Build.targets` support for the
//! NuGet reader (#655 / FU-001 from the 2026-08-04 audit).
//!
//! MSBuild automatically imports two conventional files into every
//! project during evaluation:
//!
//! - `Directory.Build.props` — imported FIRST, before the `.csproj`'s
//!   own content and the SDK props.
//! - `Directory.Build.targets` — imported LAST, after the csproj and
//!   SDK targets. Can override anything declared earlier.
//!
//! MSBuild walks from the project directory up the ancestor chain and
//! imports the NEAREST occurrence of each file (bounded by the
//! solution root or filesystem root). We match that behavior with a
//! `scan_root`-bounded walker.
//!
//! Real-world .NET projects use these files heavily to hoist common
//! declarations across many `.csproj` files. The RestSharp
//! `test/Directory.Build.props` shape from the 2026-08-04 audit is a
//! canonical example — 12 test-only `<PackageReference>` elements
//! declared once, applied to every test csproj under the `test/`
//! subtree.
//!
//! # What this module extracts
//!
//! For a discovered `Directory.Build.props` (or `.targets`) file we
//! delegate to the existing sibling parsers:
//!
//! - `csproj::parse_project_file` — `<PackageReference>` elements.
//!   Same XML shape as a .csproj so the parser is directly reusable.
//! - `directory_packages_props::parse_props` — `<PackageVersion>`
//!   elements. These extend the CPM version map (Directory.Packages.props
//!   still wins on collision — see the merge order in `mod.rs`).
//! - `msbuild_properties::parse_properties_file` — `<PropertyGroup>`
//!   contents. Feeds the FU-002 `$(PropertyName)` substitution map.
//!
//! # Scope limitations (deliberate)
//!
//! - Only the NEAREST `Directory.Build.props` / `.targets` in the
//!   ancestor chain is loaded. Chained imports via `<Import
//!   Project="$([MSBuild]::GetPathOfFileAbove(...))"/>` are ignored;
//!   matches Directory.Packages.props existing behavior.
//! - No conditional evaluation of `<Import Condition="...">`. The
//!   parsers accept the files verbatim regardless of Condition
//!   attributes on their enclosing groups.
//! - No SDK-provided implicit props/targets files (e.g., the
//!   `Microsoft.NET.Sdk`-shipped `Sdk.props`). Waybill only reads what
//!   the operator's source tree contains.

use std::path::Path;

use super::csproj::{self, NugetPackageReference};
use super::directory_packages_props::{self, CpmMap};
use super::msbuild_properties::{self, PropertyMap};

/// Discovered inputs from the nearest `Directory.Build.props` +
/// `Directory.Build.targets` in the ancestor chain. Empty vectors /
/// maps when neither file is found or parseable.
///
/// Per-reference source-file attribution lives on the
/// `NugetPackageReference::source_file` field (populated by
/// `csproj::parse_project_file`), so an aggregate `build_props_path`
/// / `build_targets_path` here would be redundant.
#[derive(Debug, Default)]
pub(super) struct DirectoryBuildFiles {
    /// `<PackageReference>` elements from both files, combined.
    /// build.props entries come first (imported before csproj), then
    /// build.targets entries (imported after). Duplicate-key handling
    /// is done downstream by the accumulator in `mod.rs`.
    pub(super) package_references: Vec<NugetPackageReference>,
    /// `<PackageVersion>` elements from both files, combined. Extends
    /// the CPM version map. Directory.Packages.props still wins on
    /// collision — this is intentional: packages.props is the
    /// canonical CPM location and operators who put a
    /// `<PackageVersion>` in Directory.Build.props typically mean it
    /// as a fallback, not an override.
    pub(super) cpm_extensions: CpmMap,
    /// `<PropertyGroup>` values from both files, combined for the
    /// FU-002 `$(PropertyName)` substitution map. build.targets
    /// overlays build.props on collision (targets is imported later).
    pub(super) property_map: PropertyMap,
}

/// Find + parse the nearest `Directory.Build.props` and
/// `Directory.Build.targets` from `start_dir` upward (bounded by
/// `scan_root`). Returns a populated [`DirectoryBuildFiles`] with
/// whichever files were found (may be one, both, or neither).
///
/// This is the single-shot entry point called by `mod.rs`
/// `read_one_project`. Read/parse failures on either individual file
/// are non-fatal (tracked via `tracing::warn!` in the delegated
/// parsers) — partial results still contribute what could be parsed.
pub(super) fn discover(start_dir: &Path, scan_root: &Path) -> DirectoryBuildFiles {
    let build_props_path = directory_packages_props::find_msbuild_file_walking_up(
        start_dir,
        scan_root,
        "Directory.Build.props",
    );
    let build_targets_path = directory_packages_props::find_msbuild_file_walking_up(
        start_dir,
        scan_root,
        "Directory.Build.targets",
    );

    let mut out = DirectoryBuildFiles::default();

    for (path_opt, is_targets) in [
        (build_props_path.as_ref(), false),
        (build_targets_path.as_ref(), true),
    ] {
        let Some(path) = path_opt else { continue };
        // Delegate to existing per-shape parsers.
        let refs = csproj::parse_project_file(path);
        let cpm = directory_packages_props::parse_props(path);
        let props = msbuild_properties::parse_properties_file(path);

        if !refs.is_empty() {
            tracing::info!(
                path = %path.display(),
                count = refs.len(),
                is_targets,
                "loaded <PackageReference> elements from Directory.Build props/targets (#655)"
            );
        }

        out.package_references.extend(refs);

        // build.targets overlays build.props for both maps (matches
        // MSBuild's later-import-wins semantics between the two).
        // `HashMap::extend` overlays existing keys with new values, so
        // inserting build.targets after build.props naturally yields
        // "targets wins on collision".
        out.cpm_extensions.extend(cpm);
        out.property_map.extend(props);
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn discover_returns_empty_when_no_build_files_present() {
        let tmp = tempfile::tempdir().unwrap();
        let out = discover(tmp.path(), tmp.path());
        assert!(out.package_references.is_empty());
        assert!(out.cpm_extensions.is_empty());
        assert!(out.property_map.is_empty());
    }

    #[test]
    fn extracts_package_references_from_build_props() {
        // RestSharp-shape: test/Directory.Build.props declares xunit
        // + coverlet.collector, and every test .csproj under this
        // subtree inherits both.
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path();
        let test_dir = scan_root.join("test");
        std::fs::create_dir_all(&test_dir).unwrap();
        write(
            &test_dir,
            "Directory.Build.props",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Xunit" Version="2.9.2" />
    <PackageReference Include="MikebomFixture.Coverlet" Version="6.0.2" />
  </ItemGroup>
</Project>"#,
        );

        // Discover from a subdirectory of test/ — walker should find
        // the props file one level up.
        let inner = test_dir.join("MyProject");
        std::fs::create_dir_all(&inner).unwrap();
        let out = discover(&inner, scan_root);

        assert_eq!(out.package_references.len(), 2);
        let includes: Vec<&str> = out
            .package_references
            .iter()
            .map(|r| r.include.as_str())
            .collect();
        assert!(includes.contains(&"MikebomFixture.Xunit"));
        assert!(includes.contains(&"MikebomFixture.Coverlet"));
    }

    #[test]
    fn extracts_package_versions_and_properties_from_build_props() {
        // Directory.Build.props can also contribute CPM PackageVersion
        // entries + PropertyGroup values.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <PropertyGroup>
    <SharedTestVer>17.12.0</SharedTestVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.TestSdk" Version="$(SharedTestVer)" />
  </ItemGroup>
</Project>"#,
        );
        let out = discover(tmp.path(), tmp.path());
        assert_eq!(
            out.cpm_extensions.get("MikebomFixture.TestSdk"),
            Some(&"$(SharedTestVer)".to_string())
        );
        assert_eq!(
            out.property_map.get("sharedtestver"),
            Some(&"17.12.0".to_string())
        );
    }

    #[test]
    fn build_targets_overlays_build_props_for_maps() {
        // build.targets is imported after build.props → later-wins on
        // both the CPM map and the property map for duplicate keys.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <PropertyGroup>
    <SharedVer>1.0.0</SharedVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.Shared" Version="1.0.0" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "Directory.Build.targets",
            r#"<Project>
  <PropertyGroup>
    <SharedVer>2.0.0</SharedVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.Shared" Version="2.0.0" />
  </ItemGroup>
</Project>"#,
        );
        let out = discover(tmp.path(), tmp.path());
        assert_eq!(out.property_map.get("sharedver"), Some(&"2.0.0".to_string()));
        assert_eq!(
            out.cpm_extensions.get("MikebomFixture.Shared"),
            Some(&"2.0.0".to_string())
        );
    }

    #[test]
    fn walker_bounds_search_at_scan_root() {
        // A Directory.Build.props ABOVE scan_root must not be found.
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path().join("project");
        let inner = scan_root.join("src");
        std::fs::create_dir_all(&inner).unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.ShouldNotBeFound" Version="1.0.0" />
  </ItemGroup>
</Project>"#,
        );
        let out = discover(&inner, &scan_root);
        assert!(out.package_references.is_empty());
        assert!(out.cpm_extensions.is_empty());
        assert!(out.property_map.is_empty());
    }
}
