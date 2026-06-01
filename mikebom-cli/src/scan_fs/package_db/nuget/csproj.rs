//! Parser for `.csproj` / `.vbproj` / `.fsproj` files (milestone 106
//! US4, FR-007).
//!
//! Schemas are identical across the three project-file extensions —
//! MSBuild handles them all via the same XML namespace. The reader
//! collects every `<PackageReference>` element (in both empty-tag and
//! paired-tag form) into a `NugetPackageReference` record.
//!
//! Per `contracts/nuget-csproj.md` — version resolution is handled
//! upstream in the dispatcher (`nuget::mod::read`) because it needs to
//! consult both the CPM map and the lockfile depending on what's
//! available alongside this project file.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::attributes::Attribute;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::private_assets::PrivateAssetAttrs;

/// One `<PackageReference>` extracted from a project file. Versions are
/// stored as `Option<String>` to distinguish "absent" (CPM-resolvable)
/// from "present but empty" (malformed source).
#[derive(Clone, Debug, Default)]
pub(super) struct NugetPackageReference {
    pub(super) include: String,
    pub(super) version: Option<String>,
    pub(super) attrs: PrivateAssetAttrs,
}

/// Parse a single `.csproj`/`.vbproj`/`.fsproj` file and return all
/// `<PackageReference>` elements found.
///
/// Returns `Vec` (empty when the file has zero `<PackageReference>`
/// elements OR when parsing fails). Parse failures emit
/// `tracing::warn!` per FR-015.
pub(super) fn parse_project_file(path: &Path) -> Vec<NugetPackageReference> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read NuGet project file (skipping; FR-015)"
            );
            return Vec::new();
        }
    };
    parse_bytes(&bytes, path)
}

fn parse_bytes(bytes: &[u8], path: &Path) -> Vec<NugetPackageReference> {
    let mut reader = Reader::from_reader(bytes);
    reader.trim_text(true);

    let mut out: Vec<NugetPackageReference> = Vec::new();
    let mut buf = Vec::new();

    // When we open a paired `<PackageReference Include="...">`, we
    // accumulate per-element state here until the matching close tag.
    let mut open_ref: Option<NugetPackageReference> = None;
    let mut current_child: Option<String> = None;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) => {
                let name = lowercase_local_name(e.name().as_ref());
                if name == "packagereference" {
                    let attrs = collect_pref_attrs(&e);
                    out.push(reference_from_attrs(attrs));
                }
            }
            Ok(Event::Start(e)) => {
                let name = lowercase_local_name(e.name().as_ref());
                if name == "packagereference" {
                    let attrs = collect_pref_attrs(&e);
                    open_ref = Some(reference_from_attrs(attrs));
                } else if open_ref.is_some() {
                    // Track child-element-form metadata
                    // (`<IncludeAssets>...</IncludeAssets>`, etc.).
                    current_child = Some(name);
                    current_text.clear();
                }
            }
            Ok(Event::Text(t)) if open_ref.is_some() && current_child.is_some() => {
                if let Ok(s) = t.unescape() {
                    current_text.push_str(s.as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let name = lowercase_local_name(e.name().as_ref());
                if name == "packagereference" {
                    if let Some(r) = open_ref.take() {
                        out.push(r);
                    }
                } else if let Some(child_name) = current_child.take() {
                    if let Some(r) = open_ref.as_mut() {
                        let val = current_text.trim().to_string();
                        if !val.is_empty() {
                            apply_child_value(r, &child_name, &val);
                        }
                    }
                    current_text.clear();
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to parse NuGet project file (skipping; FR-015)"
                );
                return Vec::new();
            }
            _ => {}
        }
        buf.clear();
    }
    out
}

fn lowercase_local_name(raw: &[u8]) -> String {
    let s = std::str::from_utf8(raw).unwrap_or("");
    let local = s.rsplit(':').next().unwrap_or(s);
    local.to_ascii_lowercase()
}

fn collect_pref_attrs(e: &quick_xml::events::BytesStart) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for attr in e.attributes().with_checks(false).flatten() {
        let Attribute { key, value } = attr;
        let key_str = std::str::from_utf8(key.as_ref())
            .unwrap_or("")
            .to_ascii_lowercase();
        let val = String::from_utf8_lossy(value.as_ref()).into_owned();
        map.insert(key_str, val);
    }
    map
}

fn reference_from_attrs(attrs: HashMap<String, String>) -> NugetPackageReference {
    NugetPackageReference {
        include: attrs.get("include").cloned().unwrap_or_default(),
        version: attrs.get("version").cloned(),
        attrs: PrivateAssetAttrs {
            private_assets: attrs.get("privateassets").cloned(),
            include_assets: attrs.get("includeassets").cloned(),
            exclude_assets: attrs.get("excludeassets").cloned(),
        },
    }
}

fn apply_child_value(r: &mut NugetPackageReference, child_name: &str, value: &str) {
    match child_name {
        "version" if r.version.is_none() => {
            r.version = Some(value.to_string());
        }
        "privateassets" if r.attrs.private_assets.is_none() => {
            r.attrs.private_assets = Some(value.to_string());
        }
        "includeassets" if r.attrs.include_assets.is_none() => {
            r.attrs.include_assets = Some(value.to_string());
        }
        "excludeassets" if r.attrs.exclude_assets.is_none() => {
            r.attrs.exclude_assets = Some(value.to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::LifecycleScope;

    fn parse(xml: &str) -> Vec<NugetPackageReference> {
        parse_bytes(xml.as_bytes(), Path::new("test.csproj"))
    }

    #[test]
    fn extracts_legacy_package_reference() {
        let xml = r#"<?xml version="1.0"?>
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />
    <PackageReference Include="MikebomFixture.OtherLib" Version="2.3.4" />
  </ItemGroup>
</Project>"#;
        let refs = parse(xml);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].include, "MikebomFixture.SampleLib");
        assert_eq!(refs[0].version.as_deref(), Some("1.2.3"));
        assert_eq!(refs[1].include, "MikebomFixture.OtherLib");
    }

    #[test]
    fn extracts_reference_without_version() {
        // CPM-style: no Version attribute, will be resolved upstream.
        let xml = r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Cpm" />
  </ItemGroup>
</Project>"#;
        let refs = parse(xml);
        assert_eq!(refs.len(), 1);
        assert!(refs[0].version.is_none());
    }

    #[test]
    fn extracts_paired_form_with_includeassets_child() {
        let xml = r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Analyzer" Version="3.4.5">
      <IncludeAssets>build;buildMultitargeting</IncludeAssets>
    </PackageReference>
  </ItemGroup>
</Project>"#;
        let refs = parse(xml);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].include, "MikebomFixture.Analyzer");
        assert_eq!(
            refs[0].attrs.include_assets.as_deref(),
            Some("build;buildMultitargeting")
        );
        // Verify it classifies as build via the private_assets module.
        assert!(matches!(
            super::super::private_assets::classify(&refs[0].attrs),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn private_assets_all_attribute_captured() {
        let xml = r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SourceLink" Version="1.0.0" PrivateAssets="All" />
  </ItemGroup>
</Project>"#;
        let refs = parse(xml);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].attrs.private_assets.as_deref(), Some("All"));
    }

    #[test]
    fn vbproj_and_fsproj_extract_identically() {
        let xml = r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SharedLib" Version="5.6.7" />
  </ItemGroup>
</Project>"#;
        // The file-extension matching happens in the dispatcher;
        // parse_bytes itself is extension-agnostic. This test just
        // verifies the parser doesn't care about the source path's
        // extension.
        let r = parse_bytes(xml.as_bytes(), Path::new("Project.vbproj"));
        assert_eq!(r.len(), 1);
        let f = parse_bytes(xml.as_bytes(), Path::new("Project.fsproj"));
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn malformed_xml_returns_empty() {
        let xml = "<Project><ItemGroup><PackageReference Include=\"x\"";
        let refs = parse(xml);
        assert!(refs.is_empty());
    }
}
