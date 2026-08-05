//! MSBuild property-map parsing + `$(PropertyName)` substitution
//! (#654 / FU-002 from the 2026-08-04 NuGet audit).
//!
//! MSBuild XML files (`.csproj`, `Directory.Packages.props`, etc.) can
//! declare properties inside `<PropertyGroup>` blocks:
//!
//! ```xml
//! <PropertyGroup>
//!   <SystemTextJsonVer>10.0.0</SystemTextJsonVer>
//! </PropertyGroup>
//! <ItemGroup>
//!   <PackageVersion Include="System.Text.Json" Version="$(SystemTextJsonVer)" />
//! </ItemGroup>
//! ```
//!
//! Prior to #654 the nuget reader treated the raw `$(SystemTextJsonVer)`
//! string as the PURL version, emitting an invalid PURL literal:
//! `pkg:nuget/System.Text.Json@$(SystemTextJsonVer)`.
//!
//! This module gives the reader two capabilities:
//! - `parse_properties` — extract all `<PropertyGroup>` child elements
//!   into a name → value map from any MSBuild XML file.
//! - `substitute` — resolve `$(PropertyName)` refs in a value string
//!   against a merged property map.
//!
//! # MSBuild semantics we handle
//!
//! - **Case-insensitive lookup**: property names are case-insensitive
//!   in MSBuild; we lowercase both the map keys and the `$()`
//!   reference-lookups.
//! - **Conditional groups**: `<PropertyGroup Condition="...">` blocks
//!   redefine values based on target-framework / OS / configuration.
//!   Without a target framework, we can't evaluate the conditions —
//!   so we take the **last-defined value** for any given property,
//!   which matches MSBuild's default evaluation order when conditions
//!   are unmet.
//!
//! # MSBuild semantics we intentionally DON'T handle
//!
//! - **Property functions**: `$([System.String]::Format(...))` — much
//!   bigger scope; if a value contains `$([...]` we return the raw
//!   string unchanged and the caller falls back to #653's design-tier
//!   path.
//! - **Item metadata**: `%(Foo.Bar)` — not used in version segments in
//!   real-world project files; unchanged.
//! - **Cross-file property imports** via `<Import Project="..."/>` —
//!   only Directory.Packages.props ancestor chain + the csproj itself
//!   contribute today. Extending to Directory.Build.props is #655.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

/// Case-insensitive property map. Keys are stored lowercased; lookups
/// via `substitute` also lowercase the reference name.
pub(super) type PropertyMap = HashMap<String, String>;

/// Read + parse a MSBuild XML file and return its property map. Empty
/// on read/parse failure (`tracing::warn!` per FR-015).
pub(super) fn parse_properties_file(path: &Path) -> PropertyMap {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read MSBuild file for property extraction (skipping; FR-015)"
            );
            return PropertyMap::new();
        }
    };
    parse_properties(&bytes, path)
}

/// Parse a MSBuild XML byte-buffer and extract all direct-child
/// elements of `<PropertyGroup>` into a lowercased-key value map.
/// Later definitions overwrite earlier ones (last-wins).
pub(super) fn parse_properties(bytes: &[u8], path: &Path) -> PropertyMap {
    let mut reader = Reader::from_reader(bytes);
    reader.trim_text(true);

    let mut map = PropertyMap::new();
    let mut buf = Vec::new();
    let mut in_property_group = false;
    let mut current_prop: Option<String> = None;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = lowercase_local_name(e.name().as_ref());
                if name == "propertygroup" {
                    in_property_group = true;
                } else if in_property_group {
                    current_prop = Some(name);
                    current_text.clear();
                }
            }
            Ok(Event::Text(t)) if in_property_group && current_prop.is_some() => {
                if let Ok(s) = t.unescape() {
                    current_text.push_str(s.as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let name = lowercase_local_name(e.name().as_ref());
                if name == "propertygroup" {
                    in_property_group = false;
                    current_prop = None;
                    current_text.clear();
                } else if let Some(prop_name) = current_prop.take() {
                    let val = current_text.trim().to_string();
                    if !val.is_empty() {
                        map.insert(prop_name, val);
                    }
                    current_text.clear();
                }
            }
            Ok(Event::Empty(_)) => {
                // Self-closing property elements are legal but rare
                // (`<Foo/>` inside PropertyGroup is equivalent to empty
                // string; we skip since substitution of "" doesn't help
                // anyone).
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to parse MSBuild file for property extraction (returning partial map; FR-015)"
                );
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    map
}

/// Substitute `$(PropertyName)` references in `value` against the
/// provided property map. Returns the substituted string.
///
/// - Missing / unknown properties are left as `$(Name)` in the output
///   — the caller detects the residual `$(` and treats the value as
///   unresolved (falls into #653's design-tier path).
/// - Property-function calls (`$([...` prefix) are opaque; we return
///   them unchanged so the caller sees them as unresolved.
/// - Nested references (`$(Foo)` where `Foo` expands to `$(Bar)`) get
///   one round of substitution and any residual `$()` triggers the
///   unresolved path — mirrors MSBuild's single-pass evaluation for
///   the common case, and avoids infinite loops on cycles.
pub(super) fn substitute(value: &str, properties: &PropertyMap) -> String {
    // Fast-path: no `$(` means no work.
    if !value.contains("$(") {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'(' {
            // Property-function form `$([Namespace]::...)` — leave
            // untouched (caller treats it as unresolved).
            if i + 2 < bytes.len() && bytes[i + 2] == b'[' {
                if let Some(close) = find_matching_close(bytes, i + 1) {
                    out.push_str(&value[i..=close]);
                    i = close + 1;
                    continue;
                } else {
                    out.push_str(&value[i..]);
                    break;
                }
            }
            // Simple property reference `$(Name)`.
            if let Some(close) = find_matching_close(bytes, i + 1) {
                let name = &value[i + 2..close];
                let key = name.to_ascii_lowercase();
                match properties.get(&key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&value[i..=close]), // preserve original
                }
                i = close + 1;
                continue;
            } else {
                out.push_str(&value[i..]);
                break;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Given a byte position pointing at `(`, return the index of the
/// matching `)`. Handles nested parens (relevant only for
/// property-function form).
fn find_matching_close(bytes: &[u8], open_pos: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open_pos;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn lowercase_local_name(raw: &[u8]) -> String {
    let s = std::str::from_utf8(raw).unwrap_or("");
    let local = s.rsplit(':').next().unwrap_or(s);
    local.to_ascii_lowercase()
}

/// Convenience: apply `substitute` and report whether the result still
/// contains any `$(` — i.e., whether at least one reference remained
/// unresolved. Used at the call site to route unresolved values into
/// the #653 design-tier path instead of emitting a broken PURL.
pub(super) fn substitute_and_check(
    value: &str,
    properties: &PropertyMap,
) -> (String, bool) {
    let subbed = substitute(value, properties);
    let has_unresolved = subbed.contains("$(");
    (subbed, has_unresolved)
}

/// Merge two property maps. Values in `overlay` take precedence over
/// values in `base` for duplicate keys — used to layer the csproj's
/// own properties on top of the closer-to-scope Directory.Packages.props
/// map (per MSBuild evaluation order where csproj is closer to the
/// consumer than an ancestor props file).
pub(super) fn merge(base: PropertyMap, overlay: PropertyMap) -> PropertyMap {
    let mut out = base;
    out.extend(overlay);
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_property_group_entries() {
        let xml = r#"<Project>
  <PropertyGroup>
    <SystemTextJsonVer>10.0.0</SystemTextJsonVer>
    <GoogleProtobufVersion>3.28.2</GoogleProtobufVersion>
  </PropertyGroup>
</Project>"#;
        let map = parse_properties(xml.as_bytes(), Path::new("Test.props"));
        assert_eq!(map.get("systemtextjsonver"), Some(&"10.0.0".to_string()));
        assert_eq!(
            map.get("googleprotobufversion"),
            Some(&"3.28.2".to_string())
        );
    }

    #[test]
    fn last_definition_wins_across_conditional_groups() {
        // Simulates MSBuild's per-target-framework `<PropertyGroup
        // Condition="'$(TargetFramework)' == 'net10.0'">` pattern. We
        // don't evaluate the Condition; we take the last-defined value
        // (matches MSBuild's default order when conditions are unmet).
        let xml = r#"<Project>
  <PropertyGroup Condition="'$(TargetFramework)' == 'net8.0'">
    <SystemTextJsonVer>8.0.0</SystemTextJsonVer>
  </PropertyGroup>
  <PropertyGroup Condition="'$(TargetFramework)' == 'net10.0'">
    <SystemTextJsonVer>10.0.0</SystemTextJsonVer>
  </PropertyGroup>
</Project>"#;
        let map = parse_properties(xml.as_bytes(), Path::new("Test.props"));
        assert_eq!(map.get("systemtextjsonver"), Some(&"10.0.0".to_string()));
    }

    #[test]
    fn substitutes_single_reference() {
        let mut map = PropertyMap::new();
        map.insert("systemtextjsonver".to_string(), "10.0.0".to_string());
        assert_eq!(substitute("$(SystemTextJsonVer)", &map), "10.0.0");
    }

    #[test]
    fn substitution_is_case_insensitive() {
        let mut map = PropertyMap::new();
        map.insert("systemtextjsonver".to_string(), "10.0.0".to_string());
        // MSBuild property names are case-insensitive.
        assert_eq!(substitute("$(systemtextjsonver)", &map), "10.0.0");
        assert_eq!(substitute("$(SYSTEMTEXTJSONVER)", &map), "10.0.0");
        assert_eq!(substitute("$(SystemTextJsonVer)", &map), "10.0.0");
    }

    #[test]
    fn substitutes_multiple_references_in_one_value() {
        let mut map = PropertyMap::new();
        map.insert("major".to_string(), "10".to_string());
        map.insert("minor".to_string(), "0".to_string());
        assert_eq!(substitute("$(Major).$(Minor).0", &map), "10.0.0");
    }

    #[test]
    fn preserves_unknown_property_as_raw() {
        let map = PropertyMap::new();
        // Missing property is left as-is so the caller can detect
        // it and fall through to design-tier.
        assert_eq!(
            substitute("$(SystemTextJsonVer)", &map),
            "$(SystemTextJsonVer)"
        );
    }

    #[test]
    fn preserves_property_function_form_untouched() {
        let map = PropertyMap::new();
        // Property-function form: `$([Namespace]::...)` — waybill
        // never evaluates these; they pass through as opaque and the
        // caller treats them as unresolved.
        let expr = "$([System.String]::Format('foo'))";
        assert_eq!(substitute(expr, &map), expr);
    }

    #[test]
    fn substitute_and_check_reports_unresolved_flag() {
        let mut map = PropertyMap::new();
        map.insert("known".to_string(), "1.0.0".to_string());

        // Fully resolved
        let (v, unresolved) = substitute_and_check("$(Known)", &map);
        assert_eq!(v, "1.0.0");
        assert!(!unresolved);

        // Unknown → left raw + flag set
        let (v, unresolved) = substitute_and_check("$(Unknown)", &map);
        assert_eq!(v, "$(Unknown)");
        assert!(unresolved);

        // Mixed: one resolved, one unresolved
        let (v, unresolved) = substitute_and_check("$(Known)-$(Missing)", &map);
        assert_eq!(v, "1.0.0-$(Missing)");
        assert!(unresolved);
    }

    #[test]
    fn substitute_no_op_when_no_dollar_paren() {
        let map = PropertyMap::new();
        assert_eq!(substitute("10.0.0", &map), "10.0.0");
        assert_eq!(substitute("", &map), "");
    }

    #[test]
    fn merge_overlay_takes_precedence() {
        let mut base = PropertyMap::new();
        base.insert("shared".to_string(), "from-base".to_string());
        base.insert("base-only".to_string(), "base".to_string());
        let mut overlay = PropertyMap::new();
        overlay.insert("shared".to_string(), "from-overlay".to_string());
        overlay.insert("overlay-only".to_string(), "overlay".to_string());
        let merged = merge(base, overlay);
        assert_eq!(merged.get("shared"), Some(&"from-overlay".to_string()));
        assert_eq!(merged.get("base-only"), Some(&"base".to_string()));
        assert_eq!(merged.get("overlay-only"), Some(&"overlay".to_string()));
    }

    #[test]
    fn regression_restsharp_directory_packages_props_scenario() {
        // Reproduces the RestSharp Directory.Packages.props shape from
        // the audit: property defined in one group, referenced in the
        // PackageVersion element in the same file. This test operates
        // on just the property extraction; the substitution + CPM
        // integration is exercised end-to-end by nuget/mod.rs tests.
        let xml = r#"<Project>
  <PropertyGroup Condition="'$(TargetFramework)' == 'net10.0'">
    <SystemTextJsonVer>10.0.0</SystemTextJsonVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="System.Text.Json" Version="$(SystemTextJsonVer)" />
  </ItemGroup>
</Project>"#;
        let props = parse_properties(xml.as_bytes(), Path::new("Directory.Packages.props"));
        let (subbed, unresolved) = substitute_and_check("$(SystemTextJsonVer)", &props);
        assert_eq!(subbed, "10.0.0");
        assert!(!unresolved);
    }

    #[test]
    fn regression_orleans_unresolved_property_scenario() {
        // Reproduces the Orleans shape from the audit: property
        // NOT defined in any parsed file (comes from an imported
        // Directory.Build.props which is FU-001, not FU-002). The
        // reference stays raw and the caller can detect it.
        let xml = r#"<Project>
  <ItemGroup>
    <PackageVersion Include="Google.Protobuf" Version="$(GoogleProtobufVersion)" />
  </ItemGroup>
</Project>"#;
        let props = parse_properties(xml.as_bytes(), Path::new("Directory.Packages.props"));
        let (subbed, unresolved) = substitute_and_check("$(GoogleProtobufVersion)", &props);
        assert_eq!(subbed, "$(GoogleProtobufVersion)");
        assert!(unresolved);
    }
}
