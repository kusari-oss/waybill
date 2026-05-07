//! CycloneDX 1.6 JSON serializer.
//!
//! Existing build logic lives in the per-section modules
//! ([`builder`], [`metadata`], [`evidence`], [`compositions`],
//! [`dependencies`], [`vex`]) — milestone 010 left that code
//! untouched. [`CycloneDxJsonSerializer`] wraps those helpers behind
//! the shared [`super::SbomSerializer`] trait so the CLI can dispatch
//! to it alongside SPDX and future formats, without changing the
//! output bytes (FR-022 / SC-006).

pub mod builder;
pub mod compositions;
pub mod dependencies;
pub mod evidence;
pub mod metadata;
pub mod serializer;
pub mod vex;

use std::path::PathBuf;

use anyhow::Context;

use super::{EmittedArtifact, OutputConfig, SbomSerializer, ScanArtifacts};
use builder::{CycloneDxBuilder, CycloneDxConfig};

/// CycloneDX 1.6 JSON serializer.
///
/// Delegates unchanged to [`CycloneDxBuilder`] and
/// `serde_json::to_string_pretty`; byte-for-byte identical to
/// pre-milestone-010 output for the same inputs (the inherently
/// volatile `serialNumber` and `metadata.timestamp` fields remain
/// generated internally, so cross-run byte-identity requires
/// normalization — see `tests/cdx_regression.rs`).
pub struct CycloneDxJsonSerializer;

impl SbomSerializer for CycloneDxJsonSerializer {
    fn id(&self) -> &'static str {
        "cyclonedx-json"
    }

    fn default_filename(&self) -> &'static str {
        "mikebom.cdx.json"
    }

    fn serialize(
        &self,
        scan: &ScanArtifacts<'_>,
        _cfg: &OutputConfig,
    ) -> anyhow::Result<Vec<EmittedArtifact>> {
        let cdx_config = CycloneDxConfig {
            include_hashes: scan.include_hashes,
            include_source_files: scan.include_source_files,
            generation_context: scan.generation_context.clone(),
            include_dev: scan.include_dev,
        };
        let builder = CycloneDxBuilder::new(cdx_config)
            .with_os_release_missing_fields(scan.os_release_missing_fields.to_vec())
            .with_go_graph_completeness(
                scan.go_graph_completeness,
                scan.go_graph_completeness_reason.map(String::from),
            )
            // Milestone 072 / T010 — propagate the source-tier SBOM
            // identifier so the metadata builder can emit the
            // standards-native `externalReferences[type:bom]` row.
            .with_source_document_binding(scan.source_document_binding.cloned())
            // Milestone 073 — propagate identifiers (built-in +
            // user-defined). Built-in identifiers ride
            // `metadata.component.externalReferences[]` per scheme;
            // user-defined identifiers ride a single
            // `metadata.properties[]` entry under
            // `mikebom:identifiers`. The Vec is already
            // deduplicated and ordered by the resolution pipeline.
            .with_identifiers(scan.identifiers.to_vec())
            // Milestone 076 — propagate per-component user-defined
            // identifiers from `--component-id <PURL>=<scheme>:<value>`
            // flags. Matched against `components[].purl` byte-equally
            // and appended as additional `properties[]` entries per
            // research §2.
            .with_component_identifiers(scan.component_identifiers.to_vec())
            // Milestone 077 — propagate operator-supplied overrides
            // for the root component's name + version. When active,
            // replaces the auto-derived metadata.component identity
            // and drops manifest-derived main-module components from
            // the emitted components[] array (clean replacement).
            .with_root_override(scan.root_override.clone())
            // Milestone 080 — propagate user-supplied SBOM metadata
            // (`--creator`, `--annotator`/`--annotation-comment`,
            // `--metadata-comment`, `--scan-target-name`,
            // `--metadata-file`). The CDX builder routes each entry
            // to its standards-native landing slot (research §2 +
            // contracts/user-sbom-metadata.md) — `Tool` to
            // `metadata.tools.components[]`, `Organization` to
            // `metadata.manufacturer`, `Person` to `metadata.authors[]`,
            // `--metadata-comment` and `--annotator`/`--annotation-comment`
            // pairs to `bom.annotations[]`, `--scan-target-name` to
            // `metadata.component.name`. SPDX 2.3 + SPDX 3 read
            // `scan.user_metadata` directly off `ScanArtifacts`; the CDX
            // builder mirrors that data into its own field so the
            // existing `build_metadata` / `build_user_annotations`
            // call sites (which were already wired to read
            // `self.user_metadata`) get populated values.
            .with_user_metadata(scan.user_metadata.clone())
            // Milestone 081 — propagate the operator-asserted CISA
            // SBOM Type override from `--sbom-type <type>`. When set,
            // CDX `metadata.lifecycles[]` collapses to a single-
            // element array via the equivalence table; per-component
            // `mikebom:sbom-tier` annotations preserve auto-detected
            // values per research §4.
            .with_sbom_type_override(scan.sbom_type_override);
        let bom = builder.build(
            scan.components,
            scan.relationships,
            scan.integrity,
            scan.target_name,
            scan.complete_ecosystems,
            scan.scan_target_coord,
        )?;
        let json_str = serde_json::to_string_pretty(&bom)
            .context("serializing CycloneDX BOM to JSON")?;
        Ok(vec![EmittedArtifact {
            relative_path: PathBuf::from(self.default_filename()),
            bytes: json_str.into_bytes(),
        }])
    }
}
