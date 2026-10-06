//! The derivation record of an edited SBOM (milestone 1071, research R7).
//!
//! Every edited document states:
//! - that it's a derivative;
//! - the hash of the original;
//! - which categories of operation were applied, and how many items each
//!   matched and changed, never the values;
//! - the original's signature.
//!
//! The record goes in the format's native "amends" link (R1) and in the
//! `waybill:derivation` annotation (catalogue row C194).
//!
//! The original's signature material is embedded so a recipient can check
//! it against the original without finding its sidecar. The exception is
//! material containing a value this edit redacted (a keyless certificate can
//! name an internal repository): it's referenced by digest instead, because
//! it can't be altered without breaking it (analysis H2).

use std::path::Path;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::redact::{contains_any, Redactor};
use super::{Format, OpReport};

pub fn sha256_hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(&Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SignatureKind {
    Jsf,
    Dsse,
    SigstoreBundle,
    #[default]
    None,
}

impl SignatureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jsf => "jsf",
            Self::Dsse => "dsse",
            Self::SigstoreBundle => "sigstore-bundle",
            Self::None => "none",
        }
    }
}

/// The original document's signature, as found next to it or in it.
#[derive(Clone, Debug, Default)]
pub struct OriginalSignature {
    pub kind: SignatureKind,
    pub material: Option<Value>,
}

impl OriginalSignature {
    /// Find the original's signature:
    /// 1. an explicit sidecar path;
    /// 2. the CycloneDX root `signature` (JSF);
    /// 3. a sidecar next to the input, named as `sbom scan` names them:
    ///    `<file>.sig.bundle.json` (Sigstore keyless) or `<file>.sig.json`
    ///    (DSSE, static key);
    /// 4. otherwise unsigned.
    pub fn find(input_path: &Path, doc: &Value, explicit: Option<&Path>) -> anyhow::Result<Self> {
        if let Some(p) = explicit {
            return Self::from_sidecar(p);
        }
        if let Some(sig) = doc.get("signature").filter(|s| s.is_object()) {
            return Ok(Self { kind: SignatureKind::Jsf, material: Some(sig.clone()) });
        }
        for suffix in [".sig.bundle.json", ".sig.json"] {
            let mut name = input_path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            name.push(suffix);
            let candidate = input_path.with_file_name(name);
            if candidate.is_file() {
                return Self::from_sidecar(&candidate);
            }
        }
        Ok(Self::default())
    }

    fn from_sidecar(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("reading the original's signature {}: {e}", path.display()))?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("the original's signature {} is not JSON: {e}", path.display()))?;
        let kind = if v.get("payloadType").is_some() && v.get("signatures").is_some() {
            SignatureKind::Dsse
        } else if v.get("verificationMaterial").is_some()
            || v.get("mediaType").and_then(Value::as_str).is_some_and(|m| m.contains("sigstore.bundle"))
        {
            SignatureKind::SigstoreBundle
        } else {
            anyhow::bail!(
                "{} is neither a DSSE envelope nor a Sigstore bundle",
                path.display()
            );
        };
        Ok(Self { kind, material: Some(v) })
    }

    /// The `original.signature` value for the record: embedded, or
    /// referenced by digest when it holds a redacted value.
    pub fn for_record(&self, redactor: &Redactor) -> Value {
        let Some(material) = &self.material else {
            return json!({ "kind": SignatureKind::None.as_str() });
        };
        let material = strip_payload(material);
        if contains_any(&material, &redactor.redacted_forms()) {
            json!({
                "kind": self.kind.as_str(),
                "embedded": false,
                "material_sha256": sha256_hex(canonical(&material).as_bytes()),
                "reason": "contains-redacted-values",
            })
        } else {
            json!({ "kind": self.kind.as_str(), "embedded": true, "material": material })
        }
    }
}

/// Signature material without the signed payload. A DSSE envelope (and a
/// DSSE-shaped Sigstore bundle) carries the whole original document as its
/// payload; embedding that would undo every drop and redaction. The
/// verifier has the original's bytes anyway: they are what it checks.
pub fn strip_payload(material: &Value) -> Value {
    let mut m = material.clone();
    if let Some(o) = m.as_object_mut() {
        o.remove("payload");
    }
    if let Some(o) = m.get_mut("dsseEnvelope").and_then(Value::as_object_mut) {
        o.remove("payload");
    }
    m
}

/// Canonical JSON text: sorted keys (serde_json's map is ordered), no
/// whitespace. Used for digests of JSON material.
pub fn canonical(v: &Value) -> String {
    v.to_string()
}

pub struct DerivationRecord {
    value: Value,
}

impl DerivationRecord {
    pub fn new(
        original_sha256: &str,
        format: Format,
        signature: Value,
        ops: Vec<OpReport>,
        ancestor: Option<Value>,
    ) -> Self {
        let mut m = Map::new();
        m.insert("schema".into(), json!("waybill-derivation/v1"));
        m.insert(
            "original".into(),
            json!({ "sha256": original_sha256, "format": format.label(), "signature": signature }),
        );
        m.insert("operations".into(), serde_json::to_value(ops).unwrap_or_else(|_| json!([])));
        m.insert("ancestors".into(), json!(ancestor.into_iter().collect::<Vec<_>>()));
        m.insert("tool".into(), json!(format!("waybill {}", env!("CARGO_PKG_VERSION"))));
        m.insert("created".into(), json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)));
        Self { value: Value::Object(m) }
    }

    pub fn to_value(&self) -> Value {
        self.value.clone()
    }
}

/// An ancestor record (the original's own), with any embedded signature
/// material that holds a value this edit redacted replaced by its digest,
/// as for the original's signature (analysis H2).
pub fn scrub_ancestor(mut record: Value, redactor: &Redactor) -> Value {
    let forms = redactor.redacted_forms();
    if let Some(sig) = record.pointer_mut("/original/signature") {
        let leaks = sig.get("material").is_some_and(|m| contains_any(m, &forms));
        if leaks {
            let kind = sig.get("kind").cloned().unwrap_or(json!("none"));
            let digest = sig.get("material").map(|m| sha256_hex(canonical(m).as_bytes())).unwrap_or_default();
            *sig = json!({
                "kind": kind,
                "embedded": false,
                "material_sha256": digest,
                "reason": "contains-redacted-values",
            });
        }
    }
    if let Some(ancestors) = record.get_mut("ancestors").and_then(Value::as_array_mut) {
        for a in ancestors.iter_mut() {
            *a = scrub_ancestor(std::mem::take(a), redactor);
        }
    }
    record
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn unsigned_original_is_recorded_as_none() {
        let s = OriginalSignature::default().for_record(&Redactor::new(None));
        assert_eq!(s, json!({"kind": "none"}));
    }

    #[test]
    fn signature_material_is_embedded_when_clean() {
        let sig = OriginalSignature { kind: SignatureKind::Jsf, material: Some(json!({"algorithm":"ES256","value":"abc"})) };
        let v = sig.for_record(&Redactor::new(None));
        assert_eq!(v["embedded"], json!(true));
        assert_eq!(v["material"]["value"], json!("abc"));
    }

    #[test]
    fn projection_ignores_original_identity() {
        let a = DerivationRecord::new("aa", Format::CycloneDx16, json!({"kind":"none"}), vec![], None).to_value();
        let b = DerivationRecord::new("bb", Format::Spdx23, json!({"kind":"jsf"}), vec![], None).to_value();
        assert_eq!(
            waybill::parity::extractors::derivation_parity_projection(&a),
            waybill::parity::extractors::derivation_parity_projection(&b)
        );
    }
}
