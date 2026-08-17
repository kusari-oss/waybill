# Wire Contract: `waybill:unresolved-reason` annotation

**Milestone**: 236

## Presence

Emitted iff and only if the component's `waybill:sbom-tier` annotation equals `"design"`. Absent on source-tier components. Absent when the component doesn't carry `waybill:sbom-tier` at all.

## Value shape

**Type**: JSON string  
**Character set**: ASCII English  
**Length**: <200 characters  
**Content**: reader-specific human-readable name of the resolution boundary (per `per-reader-strings.md` contract)

**Prohibited content** (enforced by CI substring blacklist per FR-010):

- Absolute filesystem paths (`/`, `\`, `~`, `%USERPROFILE%`, drive letters)
- Hostnames or IP addresses (any `.com` / `.net` / `.org` / etc; `192.168.x.x` etc)
- Credential-shaped substrings (`password=`, `token=`, `api_key=`, `Bearer `, etc)
- PII markers (email addresses, usernames of live users)

## Wire location per format

### CDX

Per-component `properties[]` entry:

```json
{
  "name": "waybill:unresolved-reason",
  "value": "no matching entry in Cargo.lock"
}
```

### SPDX 2.3

Per-Package `annotations[].comment` envelope (`MikebomAnnotationCommentV1`):

```json
{
  "annotationType": "OTHER",
  "annotator": "Tool: waybill",
  "annotationDate": "...",
  "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:unresolved-reason\",\"value\":\"no matching entry in Cargo.lock\"}"
}
```

### SPDX 3

Per-Package `Annotation.statement` envelope; same JSON shape as SPDX 2.3.

## Injection contract (Rust)

At each design-tier emission call-site:

```rust
extra_annotations.insert(
    "waybill:unresolved-reason".to_string(),
    serde_json::Value::String("<reason string from contract>".to_string()),
);
```

Adjacent to the existing `waybill:sbom-tier: "design"` insert. Both writes hit the same `PackageDbEntry.extra_annotations` map.

## Cross-version stability (per Q1 clarification)

- **Within a single waybill build**: byte-stable. Same fixture on same waybill binary → same reason string byte-for-byte. Enforced by per-reader unit tests.
- **Across waybill releases**: display-only. Reason strings MAY be refined for clarity without a semver-major waybill bump. Downstream tools MUST render verbatim and MUST NOT parse for programmatic branching.

## Parity contract

Registered in the m071 parity catalog with:

- Row ID: `C<next-available>` (verified at task-time)
- Label: `waybill:unresolved-reason`
- Scope: **component** (per-component annotation)
- Directionality: `SymmetricEqual` (CDX ↔ SPDX 2.3 ↔ SPDX 3)
- Order-sensitive: `false`
- Standards-native audit: **KEEP-NO-NATIVE** (per plan Principle V evaluation)
