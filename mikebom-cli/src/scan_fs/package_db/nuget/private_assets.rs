//! Classifier mapping `PrivateAssets` / `IncludeAssets` / `ExcludeAssets`
//! attributes on a `<PackageReference>` to a `LifecycleScope` (milestone
//! 106 US4, FR-007b).
//!
//! Per `contracts/nuget-csproj.md`:
//!
//! | Source attribute                                          | LifecycleScope |
//! |-----------------------------------------------------------|----------------|
//! | `PrivateAssets="All"`                                     | `Some(Build)`  |
//! | `IncludeAssets="..."` where comma-list omits `runtime`    | `Some(Build)`  |
//! | `ExcludeAssets="runtime"` (or contains `runtime`)         | `Some(Build)`  |
//! | `PrivateAssets="None"`                                    | `None`         |
//! | (attributes absent)                                       | `None`         |
//!
//! MSBuild attribute matching is case-insensitive (both attribute names
//! and values), so all comparisons normalize via `to_ascii_lowercase`.

use mikebom_common::resolution::LifecycleScope;

/// Holder for the three asset-control attributes mikebom reads off a
/// `<PackageReference>` element. Any field may be `None` if the
/// attribute was absent on the element.
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateAssetAttrs {
    pub(super) private_assets: Option<String>,
    pub(super) include_assets: Option<String>,
    pub(super) exclude_assets: Option<String>,
}

/// Map a `PackageReference`'s asset attributes to a lifecycle scope.
/// Returns `Some(LifecycleScope::Build)` when the package is build-only
/// (analyzer, source-link, MSBuild task, etc.); `None` when the package
/// flows through to the runtime classpath.
pub(super) fn classify(attrs: &PrivateAssetAttrs) -> Option<LifecycleScope> {
    if let Some(pa) = &attrs.private_assets {
        let val = pa.trim().to_ascii_lowercase();
        if val == "all" {
            return Some(LifecycleScope::Build);
        }
        if val == "none" {
            return None;
        }
        // Other PrivateAssets values (`compile`, `runtime`, etc.) fall
        // through to the IncludeAssets/ExcludeAssets gates below — they
        // narrow asset flow without making the package build-only.
    }
    if let Some(ex) = &attrs.exclude_assets {
        let tokens = comma_lowercase_tokens(ex);
        if tokens.iter().any(|t| t == "runtime") || tokens.iter().any(|t| t == "all") {
            return Some(LifecycleScope::Build);
        }
    }
    if let Some(inc) = &attrs.include_assets {
        let tokens = comma_lowercase_tokens(inc);
        // IncludeAssets is a positive list: the package flows ONLY the
        // listed asset kinds. If `runtime` isn't listed, the package
        // can't appear at runtime — build-only.
        if !tokens.is_empty() && !tokens.iter().any(|t| t == "runtime" || t == "all") {
            return Some(LifecycleScope::Build);
        }
    }
    None
}

fn comma_lowercase_tokens(value: &str) -> Vec<String> {
    value
        .split(';')
        .flat_map(|seg| seg.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn attrs(private: Option<&str>, include: Option<&str>, exclude: Option<&str>) -> PrivateAssetAttrs {
        PrivateAssetAttrs {
            private_assets: private.map(str::to_string),
            include_assets: include.map(str::to_string),
            exclude_assets: exclude.map(str::to_string),
        }
    }

    #[test]
    fn private_assets_all_is_build() {
        assert!(matches!(
            classify(&attrs(Some("All"), None, None)),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn private_assets_all_case_insensitive() {
        assert!(matches!(
            classify(&attrs(Some("aLL"), None, None)),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn private_assets_none_is_runtime() {
        assert!(classify(&attrs(Some("None"), None, None)).is_none());
    }

    #[test]
    fn no_attrs_is_runtime() {
        assert!(classify(&attrs(None, None, None)).is_none());
    }

    #[test]
    fn exclude_assets_runtime_is_build() {
        assert!(matches!(
            classify(&attrs(None, None, Some("runtime"))),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn exclude_assets_all_is_build() {
        assert!(matches!(
            classify(&attrs(None, None, Some("all"))),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn include_assets_without_runtime_is_build() {
        assert!(matches!(
            classify(&attrs(None, Some("build,buildMultitargeting"), None)),
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn include_assets_with_runtime_is_runtime() {
        assert!(classify(&attrs(None, Some("runtime,compile"), None)).is_none());
    }

    #[test]
    fn include_assets_semicolon_separator_supported() {
        assert!(matches!(
            classify(&attrs(None, Some("build;buildMultitargeting"), None)),
            Some(LifecycleScope::Build)
        ));
    }
}
