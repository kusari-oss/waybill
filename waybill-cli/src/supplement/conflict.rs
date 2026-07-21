// Milestone 119 — conflict resolution between scanner-discovered and
// operator-declared facts on the same PURL.
//
// Field-set partition per research Decision 3:
//
// - Scanner wins (bytes-derived):
//     `hashes`, `cpe`, `purl`, `version`, `binary_role`
// - Developer wins (operator-domain metadata):
//     `licenses`, `concluded_licenses`, `supplier`, `copyright`,
//     `name`, `description`, `externalReferences`
// - Catch-all default (FR-015 safety property): scanner wins.
//
// Justification derivation is mechanical: each `ConflictField` maps to
// exactly one `ConflictWinner` via `ConflictField::winner()`, which in
// turn determines the minimal-enum `justification` value emitted on the
// `waybill:assertion-conflict` annotation. No separate decision logic.

use waybill_common::resolution::ResolvedComponent;

use super::parser::SupplementComponent;

/// Fields that scanner authoritatively wins on (FR-006). Kept as a
/// const slice for documentation parity with research.md § Decision 3
/// and as a future hook for field-by-name partition queries; the
/// runtime path uses `ConflictField::winner()` directly.
#[allow(dead_code)]
pub(crate) const SCANNER_AUTHORITATIVE_FIELDS: &[&str] =
    &["hashes", "cpe", "purl", "version", "binary_role"];

/// Fields that operator authoritatively wins on (FR-007). Same caveat
/// as `SCANNER_AUTHORITATIVE_FIELDS`.
#[allow(dead_code)]
pub(crate) const DEVELOPER_AUTHORITATIVE_FIELDS: &[&str] = &[
    "licenses",
    "concluded_licenses",
    "supplier",
    "copyright",
    "name",
    "description",
    "externalReferences",
];

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Some variants are reserved for future per-field expansion.
pub(crate) enum ConflictField {
    Hashes,
    Cpe,
    Version,
    BinaryRole,
    Licenses,
    ConcludedLicenses,
    Supplier,
    Copyright,
    Name,
    Description,
    ExternalReferences,
    /// Catch-all for fields not in either authoritative set; scanner
    /// wins by default per FR-015 safety property.
    Other(String),
}

impl ConflictField {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Hashes => "hashes",
            Self::Cpe => "cpe",
            Self::Version => "version",
            Self::BinaryRole => "binary_role",
            Self::Licenses => "licenses",
            Self::ConcludedLicenses => "concluded_licenses",
            Self::Supplier => "supplier",
            Self::Copyright => "copyright",
            Self::Name => "name",
            Self::Description => "description",
            Self::ExternalReferences => "externalReferences",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Derive the winner from the field name per FR-006 / FR-007
    /// partition. The mapping is mechanical (data-model.md Entity 2
    /// invariant 1) — there's no separate stored decision.
    pub(crate) fn winner(&self) -> ConflictWinner {
        match self {
            Self::Licenses
            | Self::ConcludedLicenses
            | Self::Supplier
            | Self::Copyright
            | Self::Name
            | Self::Description
            | Self::ExternalReferences => ConflictWinner::Supplement,
            // All scanner-authoritative variants AND `Other(_)`
            // catch-all → scanner wins.
            _ => ConflictWinner::Scanner,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictWinner {
    Scanner,
    Supplement,
}

impl ConflictWinner {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Scanner => "scanner",
            Self::Supplement => "supplement",
        }
    }

    /// Minimal 2-value justification enum per spec clarification Q3 +
    /// FR-009. Derived mechanically from the winner; do NOT import
    /// OpenVEX values.
    pub(crate) fn justification(&self) -> &'static str {
        match self {
            Self::Scanner => "bytes-evident-detection-preserved",
            Self::Supplement => "developer-metadata-override",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConflictRecord {
    pub(crate) field: ConflictField,
    pub(crate) scanner_value: serde_json::Value,
    pub(crate) supplement_value: serde_json::Value,
}

impl ConflictRecord {
    pub(crate) fn winner(&self) -> ConflictWinner {
        self.field.winner()
    }

    /// JSON object emitted as one element of the
    /// `waybill:assertion-conflict` annotation's value array.
    pub(crate) fn as_json(&self) -> serde_json::Value {
        let winner = self.winner();
        serde_json::json!({
            "field": self.field.name(),
            "scanner_value": self.scanner_value.clone(),
            "supplement_value": self.supplement_value.clone(),
            "winner": winner.as_str(),
            "justification": winner.justification(),
        })
    }
}

/// Resolve a collision between a scanner-discovered component and a
/// supplement-declared component on the same canonical PURL.
///
/// For every supplement field that's present + differs from the
/// scanner-side value, build a `ConflictRecord` and apply the
/// FR-006/FR-007 partition: if the developer side wins, replace the
/// scanner's value with the developer's; otherwise, preserve the
/// scanner's value and stash the developer's as a
/// `waybill:declared-<field>` annotation.
///
/// In both directions, the losing side's value is preserved on the
/// merged component's `extra_annotations` so consumers can audit. The
/// caller (`merge::merge`) is responsible for stamping the
/// `waybill:assertion-conflict` array from the returned records.
pub(crate) fn resolve_component(
    mut scanner: ResolvedComponent,
    supplement: &SupplementComponent,
) -> (ResolvedComponent, Vec<ConflictRecord>) {
    let mut conflicts: Vec<ConflictRecord> = Vec::new();

    // licenses[] — developer wins.
    if let Some(supp_licenses) = supplement.licenses.as_ref() {
        let scanner_licenses_json = licenses_to_json(&scanner.licenses);
        let supplement_licenses_json = serde_json::Value::Array(supp_licenses.clone());
        if scanner_licenses_json != supplement_licenses_json {
            conflicts.push(ConflictRecord {
                field: ConflictField::Licenses,
                scanner_value: scanner_licenses_json.clone(),
                supplement_value: supplement_licenses_json.clone(),
            });
            // Developer wins. Preserve the scanner's prior values + the
            // supplement's verbatim CDX-shape array as annotations so
            // consumers can audit both sides.
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-licenses".to_string(),
                scanner_licenses_json,
            );
            scanner.extra_annotations.insert(
                "waybill:supplement-licenses".to_string(),
                supplement_licenses_json,
            );
            // Project the supplement's CDX-shaped license entries into
            // the typed `Vec<SpdxExpression>` field so every emission
            // path (CDX `components[]`, CDX `metadata.component`,
            // SPDX 2.3 `licenseDeclared`, SPDX 3 license elements)
            // sees the developer-declared value uniformly. Per the
            // standing scanner-side pattern (apk / rpm / maven / dpkg
            // copyright readers): try strict canonicalization first,
            // fall back to the permissive `SpdxExpression::new`. If
            // EVERY supplement license entry fails to project — e.g.,
            // the supplement carries only a `text` blob without an
            // `id` or `expression` — keep the scanner's typed Vec
            // unchanged so the annotation-only override (above) still
            // documents the operator's intent.
            let projected = project_supplement_licenses(supp_licenses);
            if !projected.is_empty() {
                scanner.licenses = projected;
            }
        }
    }

    // supplier — developer wins.
    if let Some(supp_supplier) = supplement.supplier.as_ref() {
        let scanner_val = scanner
            .supplier
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null);
        if scanner_val.as_str() != Some(supp_supplier.as_str()) {
            conflicts.push(ConflictRecord {
                field: ConflictField::Supplier,
                scanner_value: scanner_val.clone(),
                supplement_value: serde_json::Value::String(supp_supplier.clone()),
            });
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-supplier".to_string(),
                scanner_val,
            );
            scanner.supplier = Some(supp_supplier.clone());
        } else if scanner.supplier.is_none() {
            // Same value or scanner was empty + supplement filled it
            // in without conflict — treat as additive enrichment.
            scanner.supplier = Some(supp_supplier.clone());
        }
    }

    // copyright — developer wins. ResolvedComponent has no native
    // copyright field; flow through extra_annotations.
    if let Some(supp_copyright) = supplement.copyright.as_ref() {
        let scanner_val = scanner
            .extra_annotations
            .get("waybill:copyright")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let supplement_val = serde_json::Value::String(supp_copyright.clone());
        if scanner_val != supplement_val && !scanner_val.is_null() {
            conflicts.push(ConflictRecord {
                field: ConflictField::Copyright,
                scanner_value: scanner_val.clone(),
                supplement_value: supplement_val.clone(),
            });
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-copyright".to_string(),
                scanner_val,
            );
        }
        scanner
            .extra_annotations
            .insert("waybill:copyright".to_string(), supplement_val);
    }

    // name (display) — developer wins.
    if let Some(supp_name) = supplement.name.as_ref() {
        if &scanner.name != supp_name {
            conflicts.push(ConflictRecord {
                field: ConflictField::Name,
                scanner_value: serde_json::Value::String(scanner.name.clone()),
                supplement_value: serde_json::Value::String(supp_name.clone()),
            });
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-name".to_string(),
                serde_json::Value::String(scanner.name.clone()),
            );
            scanner.name = supp_name.clone();
        }
    }

    // description — developer wins. Stored via extra_annotations key.
    if let Some(supp_description) = supplement.description.as_ref() {
        let scanner_val = scanner
            .extra_annotations
            .get("waybill:description")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let supplement_val = serde_json::Value::String(supp_description.clone());
        if scanner_val != supplement_val && !scanner_val.is_null() {
            conflicts.push(ConflictRecord {
                field: ConflictField::Description,
                scanner_value: scanner_val.clone(),
                supplement_value: supplement_val.clone(),
            });
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-description".to_string(),
                scanner_val,
            );
        }
        scanner
            .extra_annotations
            .insert("waybill:description".to_string(), supplement_val);
    }

    // externalReferences — developer wins (ALL types per F1 widening).
    if let Some(supp_ext_refs) = supplement.external_references.as_ref() {
        let scanner_val = serde_json::to_value(&scanner.external_references)
            .unwrap_or(serde_json::Value::Null);
        let supplement_val = serde_json::Value::Array(supp_ext_refs.clone());
        if scanner_val != supplement_val
            && !scanner.external_references.is_empty()
        {
            conflicts.push(ConflictRecord {
                field: ConflictField::ExternalReferences,
                scanner_value: scanner_val.clone(),
                supplement_value: supplement_val.clone(),
            });
            scanner.extra_annotations.insert(
                "waybill:scanner-discovered-externalReferences".to_string(),
                scanner_val,
            );
        }
        scanner.extra_annotations.insert(
            "waybill:supplement-externalReferences".to_string(),
            supplement_val,
        );
    }

    // hashes — scanner wins (bytes-derived; FR-006 + FR-015).
    if let Some(supp_hashes) = supplement.hashes.as_ref() {
        let scanner_val =
            serde_json::to_value(&scanner.hashes).unwrap_or(serde_json::Value::Null);
        let supplement_val = serde_json::Value::Array(supp_hashes.clone());
        if scanner_val != supplement_val && !scanner.hashes.is_empty() {
            conflicts.push(ConflictRecord {
                field: ConflictField::Hashes,
                scanner_value: scanner_val,
                supplement_value: supplement_val.clone(),
            });
            // Scanner wins: stash supplement's value as
            // `waybill:declared-hashes` annotation; keep scanner's
            // typed hashes Vec untouched.
            scanner.extra_annotations.insert(
                "waybill:declared-hashes".to_string(),
                supplement_val,
            );
        } else if scanner.hashes.is_empty() {
            // Scanner had nothing; supplement adds — record as
            // annotation only (no native field promotion because we
            // can't reverse-parse arbitrary alg into ContentHash
            // without losing FR-015 fail-closed semantics).
            scanner.extra_annotations.insert(
                "waybill:declared-hashes".to_string(),
                supplement_val,
            );
        }
    }

    // cpe — scanner wins.
    if let Some(supp_cpes) = supplement.cpes.as_ref() {
        let scanner_val = serde_json::to_value(&scanner.cpes).unwrap_or(serde_json::Value::Null);
        let supplement_val =
            serde_json::Value::Array(supp_cpes.iter().map(|s| serde_json::json!(s)).collect());
        if scanner_val != supplement_val && !scanner.cpes.is_empty() {
            conflicts.push(ConflictRecord {
                field: ConflictField::Cpe,
                scanner_value: scanner_val,
                supplement_value: supplement_val.clone(),
            });
            scanner
                .extra_annotations
                .insert("waybill:declared-cpe".to_string(), supplement_val);
        }
    }

    // version — scanner wins. Same PURL → same canonical version
    // (canonical PURLs include version), so true conflicts on the
    // typed `version` field are rare; record only when the supplement
    // declared a different value.
    if let Some(supp_version) = supplement.version.as_ref() {
        if &scanner.version != supp_version {
            conflicts.push(ConflictRecord {
                field: ConflictField::Version,
                scanner_value: serde_json::Value::String(scanner.version.clone()),
                supplement_value: serde_json::Value::String(supp_version.clone()),
            });
            scanner.extra_annotations.insert(
                "waybill:declared-version".to_string(),
                serde_json::Value::String(supp_version.clone()),
            );
        }
    }

    (scanner, conflicts)
}

/// Project the supplement's CDX-shaped `licenses[]` array — each
/// entry is one of `{"license":{"id":"<SPDX-ID>"}}`,
/// `{"license":{"name":"<free-form-name>"}}`, or `{"expression":
/// "<SPDX-expr>"}` — into the typed `Vec<SpdxExpression>` form
/// the rest of the pipeline consumes. Follows the standing
/// scanner-side pattern (`SpdxExpression::try_canonical` first,
/// falling back to the permissive `SpdxExpression::new` so we
/// surface operator-declared free-form text without losing it).
///
/// Entries that fail BOTH constructors are dropped; the caller
/// detects an empty Vec and leaves the scanner's typed field
/// unchanged, falling back to annotation-only override semantics.
fn project_supplement_licenses(
    cdx_entries: &[serde_json::Value],
) -> Vec<waybill_common::types::license::SpdxExpression> {
    use waybill_common::types::license::SpdxExpression;
    let mut out: Vec<SpdxExpression> = Vec::new();
    for entry in cdx_entries {
        let raw_opt = entry
            .get("license")
            .and_then(|l| {
                l.get("id")
                    .and_then(|v| v.as_str())
                    .or_else(|| l.get("name").and_then(|v| v.as_str()))
            })
            .or_else(|| entry.get("expression").and_then(|v| v.as_str()));
        let Some(raw) = raw_opt else {
            continue;
        };
        if let Ok(expr) = SpdxExpression::try_canonical(raw) {
            out.push(expr);
        } else if let Ok(expr) = SpdxExpression::new(raw) {
            out.push(expr);
        }
    }
    out
}

/// Project the scanner's typed `licenses: Vec<SpdxExpression>` to a
/// JSON-array shape comparable with the supplement's CDX-shaped
/// licenses array. Used for the conflict-record `scanner_value` field
/// and the `waybill:scanner-discovered-licenses` annotation so
/// consumers can audit what the scanner observed pre-merge.
fn licenses_to_json(
    licenses: &[waybill_common::types::license::SpdxExpression],
) -> serde_json::Value {
    if licenses.is_empty() {
        return serde_json::Value::Array(Vec::new());
    }
    let items: Vec<serde_json::Value> = licenses
        .iter()
        .map(|l| {
            serde_json::json!({"expression": l.as_str()})
        })
        .collect();
    serde_json::Value::Array(items)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use waybill_common::types::purl::Purl;

    fn scanner_component(purl: &str) -> ResolvedComponent {
        ResolvedComponent {
            purl: Purl::new(purl).unwrap(),
            name: "scanned".to_string(),
            version: "1.0.0".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: Vec::new(),
                source_file_paths: Vec::new(),
                deps_dev_match: None,
            },
            licenses: Vec::new(),
            concluded_licenses: Vec::new(),
            hashes: Vec::new(),
            supplier: None,
            cpes: Vec::new(),
            advisories: Vec::new(),
            occurrences: Vec::new(),
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("source".to_string()),
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
            extra_annotations: std::collections::BTreeMap::new(),
            binary_role: None,
        }
    }

    fn supp_component(purl: &str) -> SupplementComponent {
        SupplementComponent {
            purl: Purl::new(purl).unwrap(),
            bom_ref: None,
            name: None,
            version: None,
            supplier: None,
            licenses: None,
            copyright: None,
            description: None,
            external_references: None,
            hashes: None,
            cpes: None,
        }
    }

    #[test]
    fn developer_license_wins_over_empty_scanner_licenses() {
        let scanner = scanner_component("pkg:cargo/opaque@1.0.0");
        let mut supp = supp_component("pkg:cargo/opaque@1.0.0");
        supp.licenses = Some(vec![serde_json::json!({"license":{"id":"Apache-2.0"}})]);
        let (merged, conflicts) = resolve_component(scanner, &supp);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field.name(), "licenses");
        assert_eq!(conflicts[0].winner().as_str(), "supplement");
        assert_eq!(
            conflicts[0].winner().justification(),
            "developer-metadata-override"
        );
        // Developer-wins value is exposed via waybill:supplement-licenses.
        assert!(merged
            .extra_annotations
            .contains_key("waybill:supplement-licenses"));
        // Follow-up: the supplement license now propagates into the
        // typed Vec<SpdxExpression> field so every emission path
        // (including Cargo's main-module → metadata.component
        // promotion) sees the operator-declared value uniformly.
        assert_eq!(merged.licenses.len(), 1);
        assert_eq!(merged.licenses[0].as_str(), "Apache-2.0");
    }

    #[test]
    fn project_supplement_licenses_handles_id_name_and_expression() {
        let entries = vec![
            serde_json::json!({"license":{"id":"MIT"}}),
            serde_json::json!({"license":{"name":"Acme Custom"}}),
            serde_json::json!({"expression":"Apache-2.0 OR MIT"}),
        ];
        let out = project_supplement_licenses(&entries);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].as_str(), "MIT");
        assert_eq!(out[1].as_str(), "Acme Custom");
        assert_eq!(out[2].as_str(), "Apache-2.0 OR MIT");
    }

    #[test]
    fn project_supplement_licenses_drops_unrecognized_entries() {
        // Entries with neither `license.id` / `license.name` nor
        // `expression` are dropped; the caller falls back to
        // annotation-only override semantics.
        let entries = vec![
            serde_json::json!({"license":{"text":"Some text-only license"}}),
            serde_json::json!({"random":"shape"}),
        ];
        let out = project_supplement_licenses(&entries);
        assert!(out.is_empty());
    }

    #[test]
    fn scanner_hashes_win_over_developer_assertion() {
        let mut scanner = scanner_component("pkg:generic/openssl@3.0.10");
        scanner.hashes.push(
            waybill_common::types::hash::ContentHash::sha256(
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            )
            .unwrap(),
        );
        let mut supp = supp_component("pkg:generic/openssl@3.0.10");
        supp.hashes = Some(vec![serde_json::json!({"alg":"SHA-256","content":"cafebabe"})]);
        let (merged, conflicts) = resolve_component(scanner, &supp);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field.name(), "hashes");
        assert_eq!(conflicts[0].winner().as_str(), "scanner");
        assert_eq!(
            conflicts[0].winner().justification(),
            "bytes-evident-detection-preserved"
        );
        // Scanner's typed hashes preserved untouched.
        assert_eq!(merged.hashes.len(), 1);
        // Supplement's declared value preserved as annotation.
        assert!(merged.extra_annotations.contains_key("waybill:declared-hashes"));
    }

    #[test]
    fn developer_name_wins_scanner_name_annotated() {
        let scanner = scanner_component("pkg:cargo/x@1.0.0");
        let mut supp = supp_component("pkg:cargo/x@1.0.0");
        supp.name = Some("BeautifulDisplayName".to_string());
        let (merged, conflicts) = resolve_component(scanner, &supp);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field.name(), "name");
        assert_eq!(merged.name, "BeautifulDisplayName");
        assert!(merged
            .extra_annotations
            .contains_key("waybill:scanner-discovered-name"));
    }

    #[test]
    fn matching_values_produce_no_conflict() {
        let scanner = scanner_component("pkg:cargo/x@1.0.0");
        let supp = supp_component("pkg:cargo/x@1.0.0");
        let (merged, conflicts) = resolve_component(scanner, &supp);
        assert_eq!(conflicts.len(), 0);
        assert_eq!(merged.name, "scanned");
    }

    #[test]
    fn justification_derived_from_field_partition() {
        // Sanity check the partition's mechanical derivation.
        assert_eq!(
            ConflictField::Licenses.winner().justification(),
            "developer-metadata-override"
        );
        assert_eq!(
            ConflictField::Hashes.winner().justification(),
            "bytes-evident-detection-preserved"
        );
        assert_eq!(
            ConflictField::Other("anything".into()).winner().justification(),
            "bytes-evident-detection-preserved"
        );
    }
}
