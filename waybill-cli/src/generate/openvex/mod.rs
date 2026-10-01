//! OpenVEX 0.2.0 JSON sidecar emitter (milestone 010).
//!
//! Emitted next to the SPDX 2.3 file when a scan produces VEX
//! statements. Cross-referenced from the SPDX document via
//! `externalDocumentRefs` with `SHA256`. Not emitted when the scan
//! produces no VEX statements (FR-016a).
//!
//! Current status: waybill's scan pipeline doesn't yet populate
//! `ResolvedComponent.advisories` anywhere — AdvisoryRef exists as
//! a data-model placeholder only. This emitter is therefore
//! scaffolding that fires a no-op for every present-day scan. The
//! moment a future milestone wires advisory discovery (OSV lookup,
//! NVD feed, etc.), the sidecar starts emitting without any change
//! to the SPDX serializer or the CLI surface.
//!
//! See [`statements`] for the typed model.

pub mod statements;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;
use data_encoding::BASE32_NOPAD;
use sha2::{Digest, Sha256};

use crate::generate::{EmittedArtifact, OutputConfig, ScanArtifacts};

use statements::{
    OpenVexDocument, OpenVexJustification, OpenVexProduct, OpenVexStatement, OpenVexStatus,
    OpenVexVulnerability, OPENVEX_CONTEXT_V0_2_0,
};

/// Default sidecar filename. Kept in lockstep with the SPDX
/// serializer's `externalDocumentRefs` entry so a consumer reading
/// only the SPDX file can locate the sidecar.
pub const OPENVEX_DEFAULT_FILENAME: &str = "waybill.openvex.json";

/// Length of the base32 prefix used in the `@id` URI — same
/// 160-bit budget as SPDX's `documentNamespace` (32 chars × 5 bits).
const ID_HASH_PREFIX_LEN: usize = 32;
const ID_BASE: &str = "https://waybill.kusari.dev/openvex/";

/// The evidence grade every backport-derived statement carries (FR-012a).
///
/// Spelled into the impact statement rather than left to a separate field
/// because OpenVEX has no grade slot, and a consumer reading only the status
/// would otherwise see `not_affected` with no indication of what it rests on.
fn grade_note(cve: &str, grade: &str) -> String {
    format!(
        "Resolved by a nixpkgs backport applied to this build. \
         The patch-to-{cve} association is {grade}: it comes from the patch \
         filename, which is evidence the maintainers believed this version \
         vulnerable, but not proof the patch fully resolves the issue."
    )
}

/// The note a declaration-derived statement carries.
///
/// Says *who* asserted it, not only how strong it is. A consumer weighing
/// this against a patch-derived claim needs the source: "nixpkgs declares"
/// and "a filename contains" are different kinds of evidence, and the grade
/// alone does not convey which is which to a reader who has not memorised
/// the vocabulary.
///
/// Carries the maintainer's own words, because on the measured sample those
/// words routinely say more than the identifier does — the reason for the
/// declaration is often the actionable part.
fn declaration_note(cve: &str, grade: &str, text: &str) -> String {
    format!(
        "nixpkgs declares this package insecure. The {cve} association is \
         {grade}: it comes from `meta.knownVulnerabilities` in the package \
         set this build pins, which is a maintainer stating that this \
         version is vulnerable — stronger than a filename match, and still a \
         claim about the version rather than about whether this build \
         reaches the vulnerable code. nixpkgs says: {text}"
    )
}

/// Whether this statement is the patch-derived `not_affected` a declaration
/// displaces.
///
/// Matches on (subcomponent, CVE) exactly. The patch-derived `affected` is
/// left alone: it is version-scoped and says something weaker but not
/// contradictory, and FR-012 withholds only the suppression.
fn is_withheld_not_affected(
    st: &OpenVexStatement,
    withheld: &std::collections::BTreeSet<(String, String)>,
) -> bool {
    if st.status != OpenVexStatus::NotAffected {
        return false;
    }
    st.products
        .first()
        .and_then(|p| p.subcomponents.first())
        .is_some_and(|sub| {
            withheld.contains(&(sub.id.clone(), st.vulnerability.name.clone()))
        })
}

/// The build's own identity — the subject of every "this build" statement.
///
/// Extracted so the backport and declaration paths cannot drift onto
/// different subjects. They make claims about one artifact, and a consumer
/// weighing a withheld `not_affected` against the `affected` that displaced
/// it is only comparing like with like if both name the same product.
fn root_component_purl(artifacts: &ScanArtifacts<'_>) -> Option<String> {
    artifacts
        .components
        .iter()
        .find(|c| {
            c.extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        })
        .map(|c| c.purl.as_str().to_string())
}

/// Statements for what nixpkgs itself declares (spec US1).
///
/// One `affected` per (component, CVE), naming the **build** as product with
/// the declared component as a subcomponent. That is the same shape milestone
/// 1035 gives its `not_affected`, and deliberately so: under the FR-012
/// reconciliation a declaration displaces that statement, and a consumer
/// comparing what they got against what they would have got should be
/// comparing like with like.
///
/// The weaker, patch-derived `affected` stays version-scoped. The document
/// therefore carries two `affected` shapes, which is the point rather than an
/// inconsistency — the stronger evidence earns the stronger subject.
fn declaration_statements(
    artifacts: &ScanArtifacts<'_>,
    root: &str,
) -> Vec<OpenVexStatement> {
    use crate::scan_fs::package_db::nix::closure::patches::EvidenceGrade;
    let Some(summary) = artifacts.nixpkgs_security_summary else {
        return Vec::new();
    };
    let grade = EvidenceGrade::NixpkgsDeclared.wire();

    summary
        .findings
        .iter()
        .map(|f| OpenVexStatement {
            vulnerability: OpenVexVulnerability {
                name: f.cve.clone(),
                description: None,
                aliases: Vec::new(),
            },
            products: vec![OpenVexProduct {
                id: root.to_string(),
                identifiers: [("purl".to_string(), root.to_string())]
                    .into_iter()
                    .collect(),
                subcomponents: vec![OpenVexProduct {
                    id: f.component_purl.clone(),
                    identifiers: [("purl".to_string(), f.component_purl.clone())]
                        .into_iter()
                        .collect(),
                    subcomponents: Vec::new(),
                }],
            }],
            status: OpenVexStatus::Affected,
            justification: None,
            impact_statement: Some(declaration_note(&f.cve, grade, &f.text)),
            action_statement: None,
        })
        .collect()
}

/// Build the two statements a backport produces (spec FR-011).
///
/// Two, never one. That nixpkgs applied a CVE-named patch is strong evidence
/// somebody believed the version vulnerable; that the patch fully resolves
/// the issue is weaker, resting on a filename. A lone `not_affected` would
/// let a consumer suppress a real finding on the weaker half, which is the
/// overclaim FR-009 exists to prevent.
///
/// They are distinguishable by subject (FR-012): the first is about the
/// component version as published, the second about the build this document
/// describes, with the component named as a subcomponent. That is the
/// canonical OpenVEX shape for "my product embeds this component and is not
/// affected", and naming the component as the *product* of the
/// `not_affected` would instead assert the version itself is clean.
fn backport_statements(artifacts: &ScanArtifacts<'_>) -> Vec<OpenVexStatement> {
    use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES;
    use crate::scan_fs::package_db::nix::closure::patches::EvidenceGrade;

    let grade = EvidenceGrade::FilenameDerived.wire();
    // Subject of the `not_affected` half: the thing being built. Without a
    // root there is nothing to say "this build" about, so the pair is not
    // emitted at all rather than half of it being emitted.
    let Some(root) = root_component_purl(artifacts) else {
        return Vec::new();
    };

    let mut by_cve: BTreeMap<String, BTreeMap<String, ()>> = BTreeMap::new();
    for c in artifacts.components {
        let Some(raw) = c
            .extra_annotations
            .get(ANN_CLOSURE_PATCHES)
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let Ok(patches) = serde_json::from_str::<serde_json::Value>(raw) else {
            continue;
        };
        for id in patches
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.get("resolves"))
            .filter_map(|r| r.as_array())
            .flatten()
            .filter_map(|i| i.get("id").and_then(|v| v.as_str()))
        {
            by_cve
                .entry(id.to_string())
                .or_default()
                .insert(c.purl.as_str().to_string(), ());
        }
    }

    let mut out = Vec::new();
    for (cve, purls) in by_cve {
        let products: Vec<OpenVexProduct> = purls
            .keys()
            .map(|purl| OpenVexProduct {
                id: purl.clone(),
                identifiers: [("purl".to_string(), purl.clone())].into_iter().collect(),
                subcomponents: Vec::new(),
            })
            .collect();
        let vuln = |name: &str| OpenVexVulnerability {
            name: name.to_string(),
            description: None,
            aliases: Vec::new(),
        };

        // 1. The version as published is affected.
        out.push(OpenVexStatement {
            vulnerability: vuln(&cve),
            products: products.clone(),
            status: OpenVexStatus::Affected,
            justification: None,
            impact_statement: Some(grade_note(&cve, grade)),
            action_statement: None,
        });
        // 2. This build is not, because the patch was applied to it.
        out.push(OpenVexStatement {
            vulnerability: vuln(&cve),
            products: vec![OpenVexProduct {
                id: root.clone(),
                identifiers: [("purl".to_string(), root.clone())].into_iter().collect(),
                subcomponents: products,
            }],
            status: OpenVexStatus::NotAffected,
            justification: Some(OpenVexJustification::VulnerableCodeNotPresent),
            impact_statement: Some(grade_note(&cve, grade)),
            action_statement: None,
        });
    }
    out
}

/// Build the OpenVEX sidecar for a scan. Returns `Ok(None)` when the
/// scan has zero advisories across every component — no file is
/// then written and the SPDX serializer skips the
/// `externalDocumentRefs` entry.
use crate::scan_fs::package_db::nix::declarations::ANN_NIXPKGS_DECLARATION;

pub fn serialize_openvex(
    artifacts: &ScanArtifacts<'_>,
    cfg: &OutputConfig,
) -> anyhow::Result<Option<EmittedArtifact>> {
    // Group advisories by id so one CVE that affects three
    // components emits one statement with three products[] — the
    // OpenVEX idiom, not three separate statements.
    //
    // Milestone 072 / T019: populate `OpenVexProduct.identifiers`
    // with the `purl` key (always — equal to the legacy `@id`
    // field). The `cyclonedx-bom-ref` and `spdx-spdxid` keys are
    // NOT populated here because at this emit-side construction
    // site we don't yet know which paired SBOM (CDX vs SPDX) the
    // sidecar will accompany — the per-format per-instance
    // identifier lookup happens at propagation time (T020) when
    // the target SBOM is known. Per `contracts/openvex-instance-
    // identifiers.md` C-1, `purl` alone is the documented baseline.
    let mut products_by_advisory: BTreeMap<String, Vec<OpenVexProduct>> =
        BTreeMap::new();
    for c in artifacts.components {
        for adv in &c.advisories {
            let purl = c.purl.as_str().to_string();
            let mut identifiers = std::collections::BTreeMap::new();
            identifiers.insert("purl".to_string(), purl.clone());
            products_by_advisory
                .entry(adv.id.clone())
                .or_default()
                .push(OpenVexProduct {
                    id: purl,
                    identifiers,
                    subcomponents: Vec::new(),
                });
        }
    }
    // Milestone 1035 (#1034, #1040): backport-derived statements, which are
    // the only ones waybill emits with a status stronger than
    // `under_investigation`.
    let backport = backport_statements(artifacts);
    // Milestone 1050: what nixpkgs itself declares. Shares the root-component
    // lookup with the backport path, so both halves of the story address the
    // same subject.
    let declared = match root_component_purl(artifacts) {
        Some(root) => declaration_statements(artifacts, &root),
        None => {
            // Every statement names the build as its product, so with no
            // main-module component there is no subject to attach one to.
            // Say so: the scan summary reports the declarations it found,
            // and a document that then contains none of them looks like the
            // read failed. Measured on a bare Nix flake carrying no language
            // manifest -- five declarations found, nothing emitted, no word
            // about it.
            let found = artifacts
                .components
                .iter()
                .filter(|c| c.extra_annotations.contains_key(ANN_NIXPKGS_DECLARATION))
                .count();
            if found > 0 {
                tracing::warn!(
                    components_with_declarations = found,
                    "nixpkgs declarations found but not emitted as VEX: this scan has no \
                     main-module component to name as the affected product. Supply \
                     --root-name/--root-version, or scan a tree with a language manifest."
                );
            }
            Vec::new()
        }
    };

    // FR-012. Where nixpkgs declares a CVE that a patch on the same component
    // also names, the declaration wins and the patch-derived `not_affected`
    // is withheld. A maintainer stating that a version is vulnerable outranks
    // a CVE read out of a filename, and emitting a `not_affected` against an
    // explicit contrary declaration would let a consumer dismiss a real
    // finding on the weaker evidence.
    //
    // Nothing is lost by withholding it: milestone 1035 records the patch in
    // the component's `pedigree.patches[]` regardless of what VEX says, so
    // the build's patching stays visible. What is suppressed is the
    // suppression, not the evidence (FR-013).
    let backport = if declared.is_empty() {
        backport
    } else {
        let withheld = artifacts
            .nixpkgs_security_summary
            .map(|s| {
                crate::scan_fs::package_db::nix::declarations::withheld_pairs(
                    artifacts.components,
                    &s.findings,
                )
            })
            .unwrap_or_default();
        backport
            .into_iter()
            .filter(|st| !is_withheld_not_affected(st, &withheld))
            .collect()
    };

    if products_by_advisory.is_empty() && backport.is_empty() && declared.is_empty() {
        return Ok(None);
    }

    // One statement per advisory id, products[] deduped within.
    // `under_investigation` is the status waybill can honestly
    // emit today — the scanner has discovered the advisory but
    // hasn't produced an impact analysis. A future milestone's VEX
    // enrichment pass will widen the status mapping.
    let statements: Vec<OpenVexStatement> = products_by_advisory
        .into_iter()
        .map(|(id, mut products)| {
            products.sort_by(|a, b| a.id.cmp(&b.id));
            products.dedup_by(|a, b| a.id == b.id);
            OpenVexStatement {
                vulnerability: OpenVexVulnerability {
                    name: id,
                    description: None,
                    aliases: Vec::new(),
                },
                products,
                status: OpenVexStatus::UnderInvestigation,
                justification: None,
                impact_statement: None,
                action_statement: None,
            }
        })
        .collect();
    let statements: Vec<OpenVexStatement> =
        statements.into_iter().chain(backport).chain(declared).collect();

    let author = format!("waybill-{}", cfg.mikebom_version);
    let timestamp = cfg
        .created
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = derive_openvex_id(artifacts, cfg.mikebom_version);

    let doc = OpenVexDocument {
        context: OPENVEX_CONTEXT_V0_2_0,
        id,
        author: author.clone(),
        timestamp,
        version: 1,
        tooling: Some(author),
        statements,
    };

    let bytes = serde_json::to_string_pretty(&doc)
        .context("serializing OpenVEX document")?
        .into_bytes();

    Ok(Some(EmittedArtifact {
        relative_path: PathBuf::from(OPENVEX_DEFAULT_FILENAME),
        bytes,
    }))
}

/// Derive a stable `@id` URI from the same inputs the SPDX
/// `documentNamespace` uses — target name + waybill version + sorted
/// component PURLs — plus a salt so the two IDs never collide.
fn derive_openvex_id(artifacts: &ScanArtifacts<'_>, mikebom_version: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"openvex-sidecar\n");
    hasher.update(b"target=");
    hasher.update(artifacts.target_name.as_bytes());
    hasher.update(b"\nmikebom=");
    hasher.update(mikebom_version.as_bytes());
    hasher.update(b"\npurls=");
    let mut purls: Vec<&str> =
        artifacts.components.iter().map(|c| c.purl.as_str()).collect();
    purls.sort_unstable();
    for p in purls {
        hasher.update(p.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let encoded = BASE32_NOPAD.encode(&digest);
    format!("{ID_BASE}{}", &encoded[..ID_HASH_PREFIX_LEN])
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::attestation::integrity::TraceIntegrity;
    use waybill_common::attestation::metadata::GenerationContext;
    use waybill_common::resolution::{
        AdvisoryRef, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::purl::Purl;

    fn mk_component(purl: &str) -> ResolvedComponent {
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: "x".to_string(),
            version: "1".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    fn empty_integrity() -> TraceIntegrity {
        TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 0,
            bloom_filter_false_positive_rate: 0.0,
            filter_categories_applied: vec![],
        }
    }

    fn mk_cfg() -> OutputConfig {
        OutputConfig {
            mikebom_version: "0.0.0-test",
            created: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            overrides: std::collections::BTreeMap::new(),
        }
    }

    fn mk_artifacts<'a>(
        comps: &'a [ResolvedComponent],
        integ: &'a TraceIntegrity,
    ) -> ScanArtifacts<'a> {
        ScanArtifacts {
            target_name: "demo",
            nixpkgs_security_summary: None,
            components: comps,
            relationships: &[],
            integrity: integ,
            complete_ecosystems: &[],
            os_release_missing_fields: &[],
            go_transitive_coverage: None,
            go_transitive_fallback_count: None,
            unresolved_declared_dep_count: 0,
            go_cache_warming: None,
            go_workspace_mode: None,
            go_toolchains_detected: None,
            cross_ecosystem_edges_report: None,
            nix_closure_summary: None,
            helm_extraction_mode: None,
            pants_resolve_summary: None,
            resolve_identity: None,
            haskell_parse_summary: None,
            gradle_scan_summary: None,
            no_binary_scan_mode: None,
            image_source: None,
            scan_target_coord: None,
            generation_context: GenerationContext::FilesystemScan,
            include_dev: false,
            include_hashes: true,
            include_source_files: false,
            // Milestone 221 US4 — test default preserves pre-m221 behavior.
            sbom_version: None,
            enrichment_degraded: None,
            scope_mode: crate::generate::ScopeMode::Artifact,
            source_document_binding: None,
            identifiers: &[],
            component_identifiers: &[],
            file_inventory_stats: None,
            nixpkgs_haskell_degraded: None,
            nix_eval_tier: None,
            nix_eval_degraded: None,
            nix_eval_system: None,
            nixpkgs_haskell_resolution: None,
            nixpkgs_haskell_closure: None,
            file_inventory_mode: None,
            file_inventory_source_shapes: None,
            root_override: crate::generate::RootComponentOverride::default(),
            preserve_manifest_main_module: false,
            user_metadata: waybill::binding::user_metadata::UserMetadata::default(),
            sbom_type_override: None,
            spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat::Full,
            collisions_summary: None,
            compiler_pipeline: None,
            project_discovery_mode: None,
        }
    }

    #[test]
    fn empty_scan_returns_none() {
        let integ = empty_integrity();
        let comps = vec![mk_component("pkg:cargo/a@1")];
        let arts = mk_artifacts(&comps, &integ);
        let result = serialize_openvex(&arts, &mk_cfg()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_with_no_components_returns_none() {
        let integ = empty_integrity();
        let arts = mk_artifacts(&[], &integ);
        let result = serialize_openvex(&arts, &mk_cfg()).unwrap();
        assert!(result.is_none());
    }

    /// A closure-derived component applying one CVE-named patch, plus the
    /// root the `not_affected` half is about.
    fn backport_scan() -> Vec<ResolvedComponent> {
        let mut patched = mk_component("pkg:generic/unzip@6.0");
        patched.extra_annotations.insert(
            crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES
                .to_string(),
            serde_json::Value::String(
                r#"[{"type":"backport","resolves":[{"type":"security","id":"CVE-2019-13232"}]},{"type":"backport"}]"#
                    .to_string(),
            ),
        );
        let mut root = mk_component("pkg:generic/the-build@1.0");
        root.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        vec![patched, root]
    }

    fn backport_statements_of(comps: &[ResolvedComponent]) -> Vec<serde_json::Value> {
        let integ = empty_integrity();
        let arts = mk_artifacts(comps, &integ);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        doc["statements"].as_array().unwrap().clone()
    }

    // ============================================================
    // Milestone 1050 — what nixpkgs itself declares
    // ============================================================

    /// A build whose closure contains a package nixpkgs declares insecure.
    ///
    /// Built by hand rather than from a fixture, and deliberately: waybill
    /// resolves declarations against plain nixpkgs, so a fixture's own
    /// packages resolve to nothing and could not exercise this at all
    /// (research R8). The shapes below are what the spec constrains.
    fn declared_summary(
        findings: &[(&str, &str, &str)],
    ) -> crate::scan_fs::package_db::nix::declarations::NixpkgsSecuritySummary {
        use crate::scan_fs::package_db::nix::declarations::{
            DeclaredFinding, NixpkgsSecuritySummary,
        };
        NixpkgsSecuritySummary {
            members_checked: 1,
            findings: findings
                .iter()
                .map(|(purl, cve, text)| DeclaredFinding {
                    component_purl: (*purl).to_string(),
                    cve: (*cve).to_string(),
                    text: (*text).to_string(),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn scan_with_declarations(
        summary: &crate::scan_fs::package_db::nix::declarations::NixpkgsSecuritySummary,
    ) -> Vec<serde_json::Value> {
        let mut root = mk_component("pkg:generic/the-build@1.0");
        root.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        let comps = [mk_component("pkg:generic/unzip@6.0"), root];
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(summary);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        doc["statements"].as_array().unwrap().clone()
    }

    #[test]
    fn a_declaration_is_attributable_to_nixpkgs_and_distinct_from_a_patch() {
        // SC-002, both halves. Asserting only the contrast with
        // patch-derived would leave "attributable to nixpkgs" untested, and
        // a statement attributed to nothing would pass that.
        let s = declared_summary(&[(
            "pkg:generic/unzip@6.0",
            "CVE-2099-0001",
            "synthetic entry for the waybill fixture",
        )]);
        let stmts = scan_with_declarations(&s);
        // CONTROL: a statement was produced at all.
        assert_eq!(stmts.len(), 1, "{stmts:#?}");

        let note = stmts[0]["impact_statement"].as_str().unwrap_or_default();
        assert!(
            note.contains("nixpkgs declares"),
            "not positively attributed to nixpkgs: {note:?}"
        );
        assert!(
            note.contains("nixpkgs-declared"),
            "carries no grade: {note:?}"
        );
        assert!(
            !note.contains("filename-derived"),
            "must be distinguishable from a patch-derived claim: {note:?}"
        );
        assert!(
            note.contains("synthetic entry"),
            "the maintainer's own words are the actionable half: {note:?}"
        );
    }

    #[test]
    fn a_declaration_names_the_build_with_the_component_as_subcomponent() {
        // FR-006a. The same shape milestone 1035 gives its `not_affected`,
        // so the FR-012 swap compares like with like.
        let s = declared_summary(&[(
            "pkg:generic/unzip@6.0",
            "CVE-2099-0001",
            "synthetic",
        )]);
        let stmts = scan_with_declarations(&s);
        assert_eq!(stmts[0]["status"], "affected");
        assert_eq!(stmts[0]["products"][0]["@id"], "pkg:generic/the-build@1.0");
        assert_eq!(
            stmts[0]["products"][0]["subcomponents"][0]["@id"],
            "pkg:generic/unzip@6.0",
            "naming the component as the product would assert the version \
             itself is affected, which is a different and broader claim"
        );
    }

    #[test]
    fn a_declaration_naming_several_identifiers_yields_one_statement_each() {
        // FR-004. One statement per identifier, not one carrying a
        // concatenated subject a consumer would have to split.
        let s = declared_summary(&[
            ("pkg:generic/unzip@6.0", "CVE-2099-0001", "first"),
            ("pkg:generic/unzip@6.0", "CVE-2099-0002", "second"),
        ]);
        let stmts = scan_with_declarations(&s);
        assert_eq!(stmts.len(), 2);
        let names: std::collections::BTreeSet<&str> = stmts
            .iter()
            .filter_map(|s| s["vulnerability"]["name"].as_str())
            .collect();
        assert_eq!(
            names,
            ["CVE-2099-0001", "CVE-2099-0002"].into_iter().collect()
        );
    }

    #[test]
    fn a_scan_with_no_declarations_adds_no_statements() {
        // The common case: a build that permitted nothing. Must not read as
        // a failure, and must not fabricate a sidecar.
        let comps = [mk_component("pkg:generic/a@1")];
        let integ = empty_integrity();
        let arts = mk_artifacts(&comps, &integ);
        assert!(
            serialize_openvex(&arts, &mk_cfg()).unwrap().is_none(),
            "no advisories, no patches and no declarations means no sidecar"
        );
    }

    /// A component both patched and declared. Carries the milestone-1035
    /// patch annotation *and* appears in the declaration findings.
    fn contested_scan() -> (Vec<ResolvedComponent>, crate::scan_fs::package_db::nix::declarations::NixpkgsSecuritySummary) {
        use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES;
        let mut patched = mk_component("pkg:generic/unzip@6.0");
        patched.extra_annotations.insert(
            ANN_CLOSURE_PATCHES.to_string(),
            serde_json::Value::String(
                r#"[{"type":"backport","resolves":[{"type":"security","id":"CVE-2099-0001"}]}]"#
                    .to_string(),
            ),
        );
        let mut root = mk_component("pkg:generic/the-build@1.0");
        root.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        let summary = declared_summary(&[(
            "pkg:generic/unzip@6.0",
            "CVE-2099-0001",
            "nixpkgs considers this version vulnerable",
        )]);
        (vec![patched, root], summary)
    }

    #[test]
    fn a_declaration_withholds_the_patch_derived_not_affected() {
        // SC-005. Both halves asserted: that the suppression is gone, AND
        // that the patch evidence survives. Checking only the first would
        // pass equally well if the pedigree entry had been dropped too,
        // which is the outcome this rule exists to avoid.
        let (comps, summary) = contested_scan();
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&summary);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        let stmts = doc["statements"].as_array().unwrap();

        let of = |st: &str| {
            stmts
                .iter()
                .filter(|s| s["status"] == st && s["vulnerability"]["name"] == "CVE-2099-0001")
                .count()
        };
        // CONTROL: statements were produced at all.
        assert!(!stmts.is_empty(), "{stmts:#?}");
        assert_eq!(
            of("not_affected"),
            0,
            "the patch-derived suppression must be withheld against an \
             explicit contrary declaration: {stmts:#?}"
        );
        assert!(
            of("affected") >= 1,
            "the declaration's affected must survive: {stmts:#?}"
        );

        // The other half of FR-013: the patch evidence is untouched. VEX
        // withholds the suppression; the pedigree still records the patch.
        let still_patched = comps.iter().any(|c| {
            c.extra_annotations
                .get(crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES)
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("CVE-2099-0001"))
        });
        assert!(
            still_patched,
            "withholding the statement must not remove the patch evidence"
        );
    }

    #[test]
    fn a_declaration_about_another_component_withholds_nothing() {
        // FR-014. Claims about different components are unrelated, and
        // neither displaces the other. Without this the rule would suppress
        // on CVE alone and silently drop a valid not_affected.
        let (comps, _) = contested_scan();
        let elsewhere = declared_summary(&[(
            "pkg:generic/something-else@1.0",
            "CVE-2099-0001",
            "different component",
        )]);
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&elsewhere);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        let stmts = doc["statements"].as_array().unwrap();
        assert!(
            stmts.iter().any(|s| s["status"] == "not_affected"),
            "a declaration about another component must not withhold this \
             one's suppression: {stmts:#?}"
        );
    }

    #[test]
    fn a_backport_produces_both_statements_with_different_subjects() {
        // Spec FR-011 / FR-012 / SC-005a. Two claims of different evidential
        // strength: that a CVE-named patch was applied is strong evidence the
        // version was believed vulnerable; that it fully resolves the issue
        // rests on a filename.
        let stmts = backport_statements_of(&backport_scan());
        // CONTROL: statements were produced at all, so the pairing assertions
        // below are not passing over an empty list.
        assert_eq!(stmts.len(), 2, "{stmts:#?}");

        let affected: Vec<&serde_json::Value> =
            stmts.iter().filter(|s| s["status"] == "affected").collect();
        let not: Vec<&serde_json::Value> =
            stmts.iter().filter(|s| s["status"] == "not_affected").collect();
        assert_eq!(affected.len(), 1);
        assert_eq!(not.len(), 1);

        // The subjects differ, so a consumer cannot collapse them (FR-012).
        assert_eq!(affected[0]["products"][0]["@id"], "pkg:generic/unzip@6.0");
        assert_eq!(not[0]["products"][0]["@id"], "pkg:generic/the-build@1.0");
        assert_eq!(
            not[0]["products"][0]["subcomponents"][0]["@id"],
            "pkg:generic/unzip@6.0",
            "the component is named as a subcomponent of the build, not as the \
             product -- naming it as the product would assert the version \
             itself is clean"
        );
        assert_eq!(not[0]["justification"], "vulnerable_code_not_present");
    }

    #[test]
    fn neither_statement_is_emitted_without_its_grade() {
        // FR-012a. An ungraded `not_affected` from a filename is exactly the
        // claim FR-009 exists to prevent: a consumer could suppress a real
        // finding on it.
        let stmts = backport_statements_of(&backport_scan());
        assert_eq!(stmts.len(), 2);
        for s in &stmts {
            let note = s["impact_statement"].as_str().unwrap_or("");
            assert!(
                note.contains("filename-derived"),
                "{} statement carries no grade: {note:?}",
                s["status"]
            );
        }
    }

    #[test]
    fn a_lone_not_affected_cannot_be_emitted() {
        // T040. The two are built together from one record, so the only way
        // to get one without the other is to change that -- which this
        // pins. Counted per CVE: every not_affected has an affected twin.
        let stmts = backport_statements_of(&backport_scan());
        let count = |st: &str| stmts.iter().filter(|s| s["status"] == st).count();
        assert_eq!(count("not_affected"), count("affected"));
        assert!(count("not_affected") > 0, "control: the pair exists at all");
    }

    #[test]
    fn a_patch_naming_no_cve_produces_no_statement_either_way() {
        // Nothing to assert about: there is no vulnerability identifier, so
        // an `affected` would name nothing and a `not_affected` would
        // suppress nothing.
        let mut c = mk_component("pkg:generic/quiet@1.0");
        c.extra_annotations.insert(
            crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES
                .to_string(),
            serde_json::Value::String(r#"[{"type":"backport"}]"#.to_string()),
        );
        let mut root = mk_component("pkg:generic/the-build@1.0");
        root.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        let integ = empty_integrity();
        let comps = [c, root];
        let arts = mk_artifacts(&comps, &integ);
        assert!(
            serialize_openvex(&arts, &mk_cfg()).unwrap().is_none(),
            "no advisories and no CVE-named patch means no sidecar"
        );
    }

    #[test]
    fn one_advisory_produces_one_statement() {
        let mut c = mk_component("pkg:cargo/a@1");
        c.advisories = vec![AdvisoryRef {
            id: "CVE-2024-1234".to_string(),
            source: "osv".to_string(),
            url: None,
        }];
        let integ = empty_integrity();
        let comps = [c];
        let arts = mk_artifacts(&comps, &integ);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        assert_eq!(
            artifact.relative_path,
            std::path::PathBuf::from(OPENVEX_DEFAULT_FILENAME)
        );
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        assert_eq!(doc["@context"], OPENVEX_CONTEXT_V0_2_0);
        assert!(doc["@id"].as_str().unwrap().starts_with(ID_BASE));
        assert_eq!(doc["version"], 1);
        assert_eq!(doc["author"], "waybill-0.0.0-test");
        let stmts = doc["statements"].as_array().unwrap();
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0]["vulnerability"]["name"], "CVE-2024-1234");
        assert_eq!(stmts[0]["status"], "under_investigation");
        assert_eq!(stmts[0]["products"][0]["@id"], "pkg:cargo/a@1");
    }

    #[test]
    fn same_cve_across_two_components_emits_one_statement_with_two_products() {
        let mut c1 = mk_component("pkg:cargo/a@1");
        let mut c2 = mk_component("pkg:cargo/b@2");
        let adv = AdvisoryRef {
            id: "CVE-2024-5555".to_string(),
            source: "osv".to_string(),
            url: None,
        };
        c1.advisories = vec![adv.clone()];
        c2.advisories = vec![adv];
        let integ = empty_integrity();
        let comps = [c1, c2];
        let arts = mk_artifacts(&comps, &integ);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        let stmts = doc["statements"].as_array().unwrap();
        assert_eq!(stmts.len(), 1, "one statement per CVE");
        let products = stmts[0]["products"].as_array().unwrap();
        assert_eq!(products.len(), 2);
        // Products are sorted alphabetically so re-runs are byte-stable.
        assert_eq!(products[0]["@id"], "pkg:cargo/a@1");
        assert_eq!(products[1]["@id"], "pkg:cargo/b@2");
    }

    #[test]
    fn two_cves_emit_two_statements_sorted_by_id() {
        let mut c = mk_component("pkg:cargo/a@1");
        c.advisories = vec![
            AdvisoryRef {
                id: "CVE-2024-0002".to_string(),
                source: "osv".to_string(),
                url: None,
            },
            AdvisoryRef {
                id: "CVE-2024-0001".to_string(),
                source: "osv".to_string(),
                url: None,
            },
        ];
        let integ = empty_integrity();
        let comps = [c];
        let arts = mk_artifacts(&comps, &integ);
        let artifact = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        let stmts = doc["statements"].as_array().unwrap();
        assert_eq!(stmts.len(), 2);
        // BTreeMap iteration is sorted, so CVE-0001 comes first.
        assert_eq!(stmts[0]["vulnerability"]["name"], "CVE-2024-0001");
        assert_eq!(stmts[1]["vulnerability"]["name"], "CVE-2024-0002");
    }

    #[test]
    fn id_is_deterministic_for_identical_inputs() {
        let mut c = mk_component("pkg:cargo/a@1");
        c.advisories = vec![AdvisoryRef {
            id: "CVE-X".to_string(),
            source: "osv".to_string(),
            url: None,
        }];
        let integ = empty_integrity();
        let comps = [c];
        let arts = mk_artifacts(&comps, &integ);
        let a = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let b = serialize_openvex(&arts, &mk_cfg()).unwrap().unwrap();
        let a_doc: serde_json::Value = serde_json::from_slice(&a.bytes).unwrap();
        let b_doc: serde_json::Value = serde_json::from_slice(&b.bytes).unwrap();
        assert_eq!(a_doc["@id"], b_doc["@id"]);
    }
}
