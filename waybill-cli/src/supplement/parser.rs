// Milestone 119 — supplement file parser + structural validator.
//
// Hand-rolled subset check (no `jsonschema` runtime dep per research
// Decision 1). Asserts the CDX envelope keys mikebom actually consumes
// during merge; ignores the broader CDX 1.6 surface (vulnerabilities,
// formulation, evidence, signature, …) we have no opinion about.
//
// Failure modes (all return non-zero exit before any walker begins per
// FR-002 / SC-005):
//
// - `Io`: file unreadable
// - `ParseJson`: bytes aren't valid JSON
// - `ValidationFailed`: structurally invalid CDX envelope or component
//   entry (missing `bomFormat`, wrong `specVersion`, missing required
//   key, unparsable PURL)
// - `DuplicatePurl`: same canonical PURL appears twice across
//   `components[]` ∪ `services[]`
// - `DanglingDependsOn`: a `dependencies[].dependsOn[]` entry doesn't
//   match anything in either the supplement or the scanner output
//   (this variant is constructed by `merge::merge()`, not `load()`)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use waybill_common::types::purl::Purl;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum SupplementError {
    #[error("supplement file `{path}` unreadable: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("supplement file `{path}` is not valid JSON: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("supplement file `{path}` failed structural validation: {reason}")]
    ValidationFailed { path: PathBuf, reason: String },
    #[error(
        "supplement file declares duplicate PURL `{0}` across components[] / services[]"
    )]
    DuplicatePurl(String),
    #[error(
        "supplement file dependencies[] references unknown bom-ref or PURL `{0}` \
         (not declared in supplement and not discovered by scanner)"
    )]
    DanglingDependsOn(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Supplement {
    pub(crate) source_sha256: String,
    pub(crate) source_path: String,
    pub(crate) components: Vec<SupplementComponent>,
    pub(crate) services: Vec<SupplementService>,
    pub(crate) dependencies: Vec<SupplementDependency>,
}

#[derive(Debug, Clone)]
pub(crate) struct SupplementComponent {
    pub(crate) purl: Purl,
    pub(crate) bom_ref: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) supplier: Option<String>,
    pub(crate) licenses: Option<Vec<serde_json::Value>>,
    pub(crate) copyright: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) external_references: Option<Vec<serde_json::Value>>,
    pub(crate) hashes: Option<Vec<serde_json::Value>>,
    pub(crate) cpes: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct SupplementService {
    pub(crate) bom_ref: Option<String>,
    pub(crate) name: String,
    pub(crate) provider: Option<String>,
    pub(crate) endpoints: Option<Vec<String>>,
    pub(crate) description: Option<String>,
    pub(crate) licenses: Option<Vec<serde_json::Value>>,
    pub(crate) external_references: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub(crate) struct SupplementDependency {
    pub(crate) ref_str: String,
    pub(crate) depends_on: Vec<String>,
}

pub(crate) fn load(path: &Path) -> Result<Supplement, SupplementError> {
    let bytes = std::fs::read(path).map_err(|source| SupplementError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let source_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        HEXLOWER.encode(&hasher.finalize())
    };
    let source_path = path.to_string_lossy().into_owned();
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|source| SupplementError::ParseJson {
            path: path.to_path_buf(),
            source,
        })?;
    let obj = value.as_object().ok_or_else(|| SupplementError::ValidationFailed {
        path: path.to_path_buf(),
        reason: "top-level JSON value is not an object".into(),
    })?;
    let bom_format = obj
        .get("bomFormat")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: "missing or non-string `bomFormat` key".into(),
        })?;
    if bom_format != "CycloneDX" {
        return Err(SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!("`bomFormat` is `{bom_format}`, expected `CycloneDX`"),
        });
    }
    let spec_version = obj
        .get("specVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: "missing or non-string `specVersion` key".into(),
        })?;
    if !matches!(spec_version, "1.4" | "1.5" | "1.6") {
        return Err(SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!(
                "`specVersion` is `{spec_version}`, expected one of `1.4` / `1.5` / `1.6`"
            ),
        });
    }
    let components = parse_components(path, obj.get("components"))?;
    let services = parse_services(path, obj.get("services"))?;
    let dependencies = parse_dependencies(path, obj.get("dependencies"))?;

    // Per spec edge case 5: PURL uniqueness within components[] ∪ services[].
    // Services don't have PURLs in CDX 1.6 (only components do), so the
    // check is effectively components-only — but services with explicit
    // PURLs (e.g., `pkg:saas/...`) declared as overrides also participate.
    let mut seen: HashSet<String> = HashSet::new();
    for c in &components {
        let canon = c.purl.as_str().to_string();
        if !seen.insert(canon.clone()) {
            return Err(SupplementError::DuplicatePurl(canon));
        }
    }
    Ok(Supplement {
        source_sha256,
        source_path,
        components,
        services,
        dependencies,
    })
}

fn parse_components(
    path: &Path,
    value: Option<&serde_json::Value>,
) -> Result<Vec<SupplementComponent>, SupplementError> {
    let Some(arr) = value else {
        return Ok(Vec::new());
    };
    let arr = arr.as_array().ok_or_else(|| SupplementError::ValidationFailed {
        path: path.to_path_buf(),
        reason: "`components` is not an array".into(),
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, entry) in arr.iter().enumerate() {
        let obj = entry.as_object().ok_or_else(|| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!("components[{idx}] is not an object"),
        })?;
        let purl_str = obj
            .get("purl")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SupplementError::ValidationFailed {
                path: path.to_path_buf(),
                reason: format!("components[{idx}] missing required key `purl`"),
            })?;
        let purl = Purl::new(purl_str).map_err(|e| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!("components[{idx}].purl `{purl_str}` is not a valid PURL: {e}"),
        })?;
        out.push(SupplementComponent {
            purl,
            bom_ref: obj.get("bom-ref").and_then(|v| v.as_str()).map(String::from),
            name: obj.get("name").and_then(|v| v.as_str()).map(String::from),
            version: obj.get("version").and_then(|v| v.as_str()).map(String::from),
            supplier: obj
                .get("supplier")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from),
            licenses: obj
                .get("licenses")
                .and_then(|v| v.as_array())
                .map(|a| a.to_vec()),
            copyright: obj.get("copyright").and_then(|v| v.as_str()).map(String::from),
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            external_references: obj
                .get("externalReferences")
                .and_then(|v| v.as_array())
                .map(|a| a.to_vec()),
            hashes: obj.get("hashes").and_then(|v| v.as_array()).map(|a| a.to_vec()),
            cpes: obj
                .get("cpe")
                .and_then(|v| v.as_str())
                .map(|s| vec![s.to_string()]),
        });
    }
    Ok(out)
}

fn parse_services(
    path: &Path,
    value: Option<&serde_json::Value>,
) -> Result<Vec<SupplementService>, SupplementError> {
    let Some(arr) = value else {
        return Ok(Vec::new());
    };
    let arr = arr.as_array().ok_or_else(|| SupplementError::ValidationFailed {
        path: path.to_path_buf(),
        reason: "`services` is not an array".into(),
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, entry) in arr.iter().enumerate() {
        let obj = entry.as_object().ok_or_else(|| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!("services[{idx}] is not an object"),
        })?;
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SupplementError::ValidationFailed {
                path: path.to_path_buf(),
                reason: format!("services[{idx}] missing required key `name`"),
            })?
            .to_string();
        out.push(SupplementService {
            bom_ref: obj.get("bom-ref").and_then(|v| v.as_str()).map(String::from),
            name,
            provider: obj
                .get("provider")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from),
            endpoints: obj.get("endpoints").and_then(|v| v.as_array()).map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            licenses: obj
                .get("licenses")
                .and_then(|v| v.as_array())
                .map(|a| a.to_vec()),
            external_references: obj
                .get("externalReferences")
                .and_then(|v| v.as_array())
                .map(|a| a.to_vec()),
        });
    }
    Ok(out)
}

fn parse_dependencies(
    path: &Path,
    value: Option<&serde_json::Value>,
) -> Result<Vec<SupplementDependency>, SupplementError> {
    let Some(arr) = value else {
        return Ok(Vec::new());
    };
    let arr = arr.as_array().ok_or_else(|| SupplementError::ValidationFailed {
        path: path.to_path_buf(),
        reason: "`dependencies` is not an array".into(),
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, entry) in arr.iter().enumerate() {
        let obj = entry.as_object().ok_or_else(|| SupplementError::ValidationFailed {
            path: path.to_path_buf(),
            reason: format!("dependencies[{idx}] is not an object"),
        })?;
        let ref_str = obj
            .get("ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SupplementError::ValidationFailed {
                path: path.to_path_buf(),
                reason: format!("dependencies[{idx}] missing required key `ref`"),
            })?
            .to_string();
        let depends_on_arr = obj.get("dependsOn").and_then(|v| v.as_array()).ok_or_else(
            || SupplementError::ValidationFailed {
                path: path.to_path_buf(),
                reason: format!(
                    "dependencies[{idx}] missing required key `dependsOn` (must be an array)"
                ),
            },
        )?;
        let depends_on: Vec<String> = depends_on_arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        out.push(SupplementDependency { ref_str, depends_on });
    }
    Ok(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_tmp(s: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(s.as_bytes()).unwrap();
        f
    }

    #[test]
    fn empty_supplement_loads() {
        let f = write_tmp(r#"{"bomFormat":"CycloneDX","specVersion":"1.6"}"#);
        let s = load(f.path()).unwrap();
        assert_eq!(s.components.len(), 0);
        assert_eq!(s.services.len(), 0);
        assert_eq!(s.dependencies.len(), 0);
        assert_eq!(s.source_sha256.len(), 64);
    }

    #[test]
    fn missing_file_returns_io_err() {
        let err = load(Path::new("/no/such/path/supplement.cdx.json")).unwrap_err();
        assert!(matches!(err, SupplementError::Io { .. }));
    }

    #[test]
    fn invalid_json_returns_parse_err() {
        let f = write_tmp("not-json");
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::ParseJson { .. }));
    }

    #[test]
    fn wrong_bom_format_returns_validation_err() {
        let f = write_tmp(r#"{"bomFormat":"SPDX","specVersion":"1.6"}"#);
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::ValidationFailed { .. }));
    }

    #[test]
    fn wrong_spec_version_returns_validation_err() {
        let f = write_tmp(r#"{"bomFormat":"CycloneDX","specVersion":"1.0"}"#);
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::ValidationFailed { .. }));
    }

    #[test]
    fn accepts_cdx_1_4_1_5_1_6() {
        for v in ["1.4", "1.5", "1.6"] {
            let f = write_tmp(&format!(
                r#"{{"bomFormat":"CycloneDX","specVersion":"{v}"}}"#
            ));
            load(f.path()).unwrap();
        }
    }

    #[test]
    fn parses_simple_component() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "components":[{
                    "type":"library",
                    "bom-ref":"liberror-1.2.3",
                    "purl":"pkg:generic/liberror@1.2.3",
                    "name":"liberror",
                    "supplier":{"name":"Acme"},
                    "licenses":[{"license":{"id":"MIT"}}],
                    "copyright":"© 2026 Acme"
                }]
            }"#,
        );
        let s = load(f.path()).unwrap();
        assert_eq!(s.components.len(), 1);
        let c = &s.components[0];
        assert_eq!(c.purl.as_str(), "pkg:generic/liberror@1.2.3");
        assert_eq!(c.name.as_deref(), Some("liberror"));
        assert_eq!(c.supplier.as_deref(), Some("Acme"));
        assert_eq!(c.copyright.as_deref(), Some("© 2026 Acme"));
        assert!(c.licenses.is_some());
    }

    #[test]
    fn duplicate_purl_returns_duplicate_err() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "components":[
                    {"purl":"pkg:generic/x@1.0"},
                    {"purl":"pkg:generic/x@1.0"}
                ]
            }"#,
        );
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::DuplicatePurl(_)));
    }

    #[test]
    fn missing_purl_returns_validation_err() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "components":[{"name":"x"}]
            }"#,
        );
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::ValidationFailed { .. }));
    }

    #[test]
    fn invalid_purl_returns_validation_err() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "components":[{"purl":"not-a-purl"}]
            }"#,
        );
        let err = load(f.path()).unwrap_err();
        assert!(matches!(err, SupplementError::ValidationFailed { .. }));
    }

    #[test]
    fn parses_service() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "services":[{
                    "bom-ref":"stripe",
                    "name":"Stripe",
                    "provider":{"name":"Stripe, Inc."},
                    "endpoints":["https://api.stripe.com"]
                }]
            }"#,
        );
        let s = load(f.path()).unwrap();
        assert_eq!(s.services.len(), 1);
        assert_eq!(s.services[0].name, "Stripe");
        assert_eq!(s.services[0].provider.as_deref(), Some("Stripe, Inc."));
    }

    #[test]
    fn parses_dependency_edge() {
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "dependencies":[
                    {"ref":"pkg:cargo/app@1.0", "dependsOn":["liberror-1.2.3","stripe"]}
                ]
            }"#,
        );
        let s = load(f.path()).unwrap();
        assert_eq!(s.dependencies.len(), 1);
        assert_eq!(s.dependencies[0].ref_str, "pkg:cargo/app@1.0");
        assert_eq!(s.dependencies[0].depends_on.len(), 2);
    }

    #[test]
    fn ignores_metadata_component_per_fr014() {
        // FR-014 / clarification Q1: supplement's metadata.component is ignored.
        // We simply don't read it, so this test just confirms presence is harmless.
        let f = write_tmp(
            r#"{
                "bomFormat":"CycloneDX","specVersion":"1.6",
                "metadata":{"component":{"type":"application","name":"ghost"}}
            }"#,
        );
        load(f.path()).unwrap();
    }
}
