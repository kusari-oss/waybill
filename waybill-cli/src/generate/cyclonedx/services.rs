// Milestone 119 (#326) — CDX 1.6 `services[]` emitter.
//
// CDX 1.6 has a native `services[]` section the scanner has never
// populated (no on-disk evidence pattern signals a SaaS service). The
// supplement file is the operator's mechanism for declaring these
// entries; this module renders them into the emitted CDX JSON.
//
// CDX 1.6 service entries DO NOT carry a `type` field (the value
// lives only on `components[]`). See the CDX 1.6 schema —
// `Service.bom-ref / name / provider / endpoints / description /
// licenses / externalReferences` are the honored fields.

use serde_json::{json, Value};

use crate::supplement::SupplementService;

/// Build the CDX 1.6 `services[]` JSON array from the supplement's
/// declared service entries. Returns `Value::Null` when the input
/// slice is empty so the emitter can OMIT the field entirely — that
/// preserves byte-identity with pre-119 mikebom output when no
/// supplement was supplied (FR-013 / SC-006).
pub(super) fn build_services(services: &[SupplementService]) -> Value {
    if services.is_empty() {
        return Value::Null;
    }
    let entries: Vec<Value> = services.iter().map(build_one).collect();
    json!(entries)
}

fn build_one(s: &SupplementService) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(bref) = &s.bom_ref {
        obj.insert("bom-ref".to_string(), Value::String(bref.clone()));
    }
    obj.insert("name".to_string(), Value::String(s.name.clone()));
    if let Some(provider) = &s.provider {
        obj.insert("provider".to_string(), json!({ "name": provider }));
    }
    if let Some(endpoints) = &s.endpoints {
        if !endpoints.is_empty() {
            obj.insert(
                "endpoints".to_string(),
                Value::Array(
                    endpoints
                        .iter()
                        .map(|e| Value::String(e.clone()))
                        .collect(),
                ),
            );
        }
    }
    if let Some(desc) = &s.description {
        obj.insert("description".to_string(), Value::String(desc.clone()));
    }
    if let Some(licenses) = &s.licenses {
        if !licenses.is_empty() {
            obj.insert("licenses".to_string(), Value::Array(licenses.clone()));
        }
    }
    if let Some(ext_refs) = &s.external_references {
        if !ext_refs.is_empty() {
            obj.insert(
                "externalReferences".to_string(),
                Value::Array(ext_refs.clone()),
            );
        }
    }
    // FR-011 / contracts/annotation-shape.md § Annotation 1:
    // every supplement-declared entry carries `mikebom:source-tier =
    // "declared"`. Services live outside the per-component
    // ResolvedComponent channel, so the stamp emits directly here
    // via the CDX-native `properties[]` slot on Service.
    obj.insert(
        "properties".to_string(),
        json!([{ "name": "mikebom:source-tier", "value": "declared" }]),
    );
    Value::Object(obj)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::supplement::SupplementService;

    fn svc(name: &str) -> SupplementService {
        SupplementService {
            bom_ref: Some(format!("{name}-ref")),
            name: name.to_string(),
            provider: None,
            endpoints: None,
            description: None,
            licenses: None,
            external_references: None,
        }
    }

    #[test]
    fn empty_input_returns_null() {
        let v = build_services(&[]);
        assert!(v.is_null());
    }

    #[test]
    fn single_service_emits_required_fields() {
        let v = build_services(&[svc("Stripe")]);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let s = &arr[0];
        assert_eq!(s.get("name").unwrap().as_str(), Some("Stripe"));
        assert_eq!(s.get("bom-ref").unwrap().as_str(), Some("Stripe-ref"));
        // No `type` field per CDX 1.6 spec — services[] doesn't carry it.
        assert!(s.get("type").is_none());
        // mikebom:source-tier=declared property present.
        let props = s.get("properties").unwrap().as_array().unwrap();
        assert!(props.iter().any(|p| p.get("name").unwrap().as_str()
            == Some("mikebom:source-tier")
            && p.get("value").unwrap().as_str() == Some("declared")));
    }

    #[test]
    fn provider_emits_as_nested_object() {
        let mut s = svc("Twilio");
        s.provider = Some("Twilio, Inc.".to_string());
        let v = build_services(&[s]);
        let arr = v.as_array().unwrap();
        let prov = arr[0].get("provider").unwrap();
        assert_eq!(prov.get("name").unwrap().as_str(), Some("Twilio, Inc."));
    }

    #[test]
    fn endpoints_emit_as_string_array() {
        let mut s = svc("Stripe");
        s.endpoints = Some(vec!["https://api.stripe.com".to_string()]);
        let v = build_services(&[s]);
        let arr = v.as_array().unwrap();
        let endpoints = arr[0].get("endpoints").unwrap().as_array().unwrap();
        assert_eq!(endpoints[0].as_str(), Some("https://api.stripe.com"));
    }
}
