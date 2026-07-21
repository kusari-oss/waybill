//! Parser for NuGet's `packages.lock.json` (milestone 106 US4, FR-008).
//!
//! When `<RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>`
//! is set in a project file, `dotnet restore` writes a lockfile
//! enumerating the resolved version of every direct and transitive
//! dependency per target framework. waybill prefers this lockfile over
//! the `.csproj`'s `Version=` attribute (which is usually a range like
//! `[13.0.3, )` while the lockfile gives the pinned version), and
//! additionally emits transitive components the `.csproj` alone
//! wouldn't surface.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// `dependencies.<framework>.<name>` entry. Only the fields waybill
/// consumes are typed; everything else is dropped via `serde`'s default
/// "ignore-unknown-fields" behavior.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LockedPackage {
    /// `"Direct"`, `"Transitive"`, or `"Project"`. Unknown values are
    /// preserved verbatim so the dispatcher can surface them via
    /// annotations without re-validation.
    #[serde(rename = "type", default)]
    pub(super) entry_type: String,
    #[serde(default)]
    pub(super) resolved: String,
    /// Direct deps of this package within the resolved tree (name →
    /// version-range string). Used to emit `dependsOn` edges.
    #[serde(default)]
    pub(super) dependencies: BTreeMap<String, String>,
}

/// Full lockfile shape: `dependencies.<framework>.<name> → LockedPackage`.
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct PackagesLockFile {
    #[serde(default)]
    pub(super) dependencies: BTreeMap<String, BTreeMap<String, LockedPackage>>,
}

/// Parse a `packages.lock.json`. Returns `None` on read or parse
/// failure (warns via `tracing::warn!`); the dispatcher then falls
/// back to .csproj / CPM resolution.
pub(super) fn parse(path: &Path) -> Option<PackagesLockFile> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read packages.lock.json (skipping; FR-015)"
            );
            return None;
        }
    };
    match serde_json::from_slice::<PackagesLockFile>(&bytes) {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to parse packages.lock.json (skipping; FR-015)"
            );
            None
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_direct_and_transitive() {
        let json = r#"{
            "version": 1,
            "dependencies": {
                "net8.0": {
                    "MikebomFixture.SampleLib": {
                        "type": "Direct",
                        "requested": "[1.2.3, )",
                        "resolved": "1.2.3",
                        "contentHash": "aaa",
                        "dependencies": {
                            "MikebomFixture.SubDep": "4.5.1"
                        }
                    },
                    "MikebomFixture.SubDep": {
                        "type": "Transitive",
                        "resolved": "4.5.1",
                        "contentHash": "bbb"
                    }
                }
            }
        }"#;
        let lock: PackagesLockFile = serde_json::from_str(json).unwrap();
        let net8 = lock.dependencies.get("net8.0").unwrap();
        let direct = net8.get("MikebomFixture.SampleLib").unwrap();
        assert_eq!(direct.entry_type, "Direct");
        assert_eq!(direct.resolved, "1.2.3");
        assert_eq!(direct.dependencies.len(), 1);
        let transitive = net8.get("MikebomFixture.SubDep").unwrap();
        assert_eq!(transitive.entry_type, "Transitive");
    }

    #[test]
    fn multi_target_framework_dependencies_separated() {
        let json = r#"{
            "dependencies": {
                "net6.0": {
                    "MikebomFixture.MultiTarget": {
                        "type": "Direct",
                        "resolved": "1.0.0"
                    }
                },
                "net8.0": {
                    "MikebomFixture.MultiTarget": {
                        "type": "Direct",
                        "resolved": "2.0.0"
                    }
                }
            }
        }"#;
        let lock: PackagesLockFile = serde_json::from_str(json).unwrap();
        assert_eq!(lock.dependencies.len(), 2);
        let net6 = lock.dependencies.get("net6.0").unwrap();
        let net8 = lock.dependencies.get("net8.0").unwrap();
        assert_eq!(
            net6.get("MikebomFixture.MultiTarget").unwrap().resolved,
            "1.0.0"
        );
        assert_eq!(
            net8.get("MikebomFixture.MultiTarget").unwrap().resolved,
            "2.0.0"
        );
    }

    #[test]
    fn empty_lockfile_parses_cleanly() {
        let lock: PackagesLockFile =
            serde_json::from_str(r#"{"version":1,"dependencies":{}}"#).unwrap();
        assert!(lock.dependencies.is_empty());
    }
}
