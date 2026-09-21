//! Milestone 924 (#932) — FR-007/FR-008: naming ecosystems waybill cannot read.
//!
//! **Marker-first, never extension-first** (FR-008). A directory's marker
//! often sits above the source it governs — a `mix.exs` at a project root with
//! every `.ex` three levels down — so attributing by extension mislabels
//! exactly the layouts this report exists to explain. Extension histograms are
//! recorded as observation only.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::schema::{EcosystemAttribution, EcosystemSupport};

const TABLE: &str = include_str!("ecosystems.data");

/// marker filename → ecosystem name, for ecosystems with no reader.
pub(crate) fn unsupported_markers() -> &'static BTreeMap<String, String> {
    static T: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    T.get_or_init(|| {
        TABLE
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let mut parts = l.split('\t');
                let marker = parts.next()?.trim();
                let eco = parts.next()?.trim();
                (!marker.is_empty() && !eco.is_empty())
                    .then(|| (marker.to_string(), eco.to_string()))
            })
            .collect()
    })
}

/// Attribute a directory from the marker files it holds.
///
/// `claimed` says whether any reader took a file here — which is how a
/// *supported* ecosystem is recognised. A marker in the unsupported table
/// yields `NoReader`; a marker waybill does read yields `Supported`.
///
/// Returns every ecosystem observed and marks none authoritative (FR-013).
pub(crate) fn attribute(filenames: &std::collections::BTreeSet<String>, claimed: bool)
    -> Vec<EcosystemAttribution>
{
    let table = unsupported_markers();
    let mut out = Vec::new();

    for name in filenames {
        if let Some(eco) = table.get(name.as_str()) {
            out.push(EcosystemAttribution {
                ecosystem: eco.clone(),
                evidence_marker: name.clone(),
                support: EcosystemSupport::NoReader,
            });
        } else if claimed && super::significance::is_marker(name) {
            // A marker waybill's readers did engage with. Named so the report
            // distinguishes "supported and produced nothing" from "no reader
            // exists" — different problems, opposite responses (spec US2 §2).
            out.push(EcosystemAttribution {
                ecosystem: ecosystem_for_marker(name).to_string(),
                evidence_marker: name.clone(),
                support: EcosystemSupport::Supported,
            });
        }
    }
    out
}

/// Best-known ecosystem name for a marker waybill reads. Coarse on purpose:
/// the report's job here is to say *which* project shape was seen, not to
/// re-derive the reader's own classification.
pub(crate) fn ecosystem_for_marker(name: &str) -> &'static str {
    match name {
        "go.mod" | "go.sum" => "go",
        "Cargo.toml" => "cargo",
        "package.json" => "npm",
        "pom.xml" => "maven",
        n if n.starts_with("build.gradle") || n.starts_with("settings.gradle") => "gradle",
        "pyproject.toml" | "setup.py" | "requirements.txt" | "uv.lock" => "python",
        "Gemfile" | "Gemfile.lock" => "rubygems",
        "composer.json" => "composer",
        "mix.exs" => "elixir",
        "rebar.config" => "erlang",
        "pubspec.yaml" => "dart",
        "Package.swift" => "swift",
        "Podfile" => "cocoapods",
        "build.sbt" => "scala",
        "stack.yaml" => "haskell",
        "CMakeLists.txt" => "cmake",
        "conanfile.txt" | "conanfile.py" => "conan",
        "vcpkg.json" => "vcpkg",
        "pants.toml" => "pants",
        n if n.ends_with(".cabal") => "haskell",
        n if n.ends_with(".csproj") => "nuget",
        n if n.starts_with("WORKSPACE") || n.starts_with("BUILD") || n.starts_with("MODULE") => "bazel",
        _ => "unknown",
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_table_parses_and_is_not_empty() {
        let t = unsupported_markers();
        assert!(t.len() >= 8, "seeded table should hold the R6 entries: {}", t.len());
        assert_eq!(t.get("deno.json").map(String::as_str), Some("deno"));
        assert_eq!(t.get("Project.toml").map(String::as_str), Some("julia"));
    }

    /// FR-008. A directory of source files with no marker gets no attribution,
    /// however suggestive the extensions are.
    #[test]
    fn extensions_alone_never_produce_an_attribution() {
        let files: BTreeSet<String> =
            ["main.zig", "util.zig", "README.md"].iter().map(|s| s.to_string()).collect();
        assert!(
            attribute(&files, false).is_empty(),
            "a directory of .zig files with no build.zig must NOT be called zig",
        );
    }

    #[test]
    fn an_unsupported_marker_is_named_with_no_reader() {
        let files: BTreeSet<String> = ["deno.json"].iter().map(|s| s.to_string()).collect();
        let a = attribute(&files, false);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].ecosystem, "deno");
        assert_eq!(a[0].support, EcosystemSupport::NoReader);
        assert_eq!(a[0].evidence_marker, "deno.json");
    }

    /// FR-013 — every ecosystem observed, none marked authoritative.
    #[test]
    fn several_markers_yield_several_attributions_none_preferred() {
        let files: BTreeSet<String> =
            ["deno.json", "Project.toml", "shard.yml"].iter().map(|s| s.to_string()).collect();
        let a = attribute(&files, false);
        assert_eq!(a.len(), 3, "all three must be named: {a:?}");
        let ecos: BTreeSet<&str> = a.iter().map(|x| x.ecosystem.as_str()).collect();
        assert!(ecos.contains("deno") && ecos.contains("julia") && ecos.contains("crystal"));
    }
}
