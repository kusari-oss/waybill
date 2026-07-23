# Contract: `SplitEntry.members` additive-optional schema

**Feature**: 219-split-modes | **Related**: FR-005 (locked per Q1 clarification), SC-005

## Surface

Extend the m215 `SplitEntry` struct at `waybill-cli/src/generate/split_manifest.rs:32` with a NEW additive-optional field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitEntry {
    // ... m215 fields UNCHANGED ...
    pub subproject_id: String,
    pub root_purl: String,
    pub source_dir: String,
    pub component_count: u64,
    pub shared_deps_count: u64,
    pub files: BTreeMap<String, String>,
    // NEW m219 field:
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub members: Option<Vec<SplitMember>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitMember {
    pub purl: String,
    pub source_dir: String,
}
```

## Emission rules

- **Single-member group** (`group.members.len() == 1`): `members = None`. `serde_json` omits the field entirely (via `skip_serializing_if = "Option::is_none"`). Wire shape IS byte-identical to m215 alpha.67.
- **Multi-member group** (`group.members.len() >= 2`): `members = Some(vec)` where `vec` is populated with one `SplitMember` per contributing main-module, sorted lex by `SplitMember::purl`. Wire shape includes the `"members": [...]` field.

**Invariant enforcement**: the emit-time populator in `emit_split` enforces the "0-or-≥2 members" contract:

```rust
let members = if group.members.len() >= 2 {
    let mut vec: Vec<SplitMember> = group.members.iter().map(|m| SplitMember {
        purl: m.purl_string.clone(),
        source_dir: m.source_dir.to_string_lossy().to_string(),
    }).collect();
    vec.sort_by(|a, b| a.purl.cmp(&b.purl));
    Some(vec)
} else {
    None
};
```

## Wire shapes

### Single-member group (byte-identical to m215)

```json
{
  "subproject_id": "libsafe.cargo",
  "root_purl": "pkg:cargo/libsafe@0.1.0",
  "source_dir": "crates/libsafe",
  "component_count": 42,
  "shared_deps_count": 3,
  "files": {"cyclonedx-json": "libsafe.cargo.cdx.json"}
}
```

No `"members"` field. `jq '.members'` returns `null`. Deserialize into m219 `SplitEntry` → `members: None` (via `#[serde(default)]`).

### Multi-member group (m219 new)

```json
{
  "subproject_id": "services-api.multi",
  "root_purl": "pkg:generic/services-api@0.0.0-unknown",
  "source_dir": "services/api",
  "component_count": 123,
  "shared_deps_count": 5,
  "files": {"cyclonedx-json": "services-api.multi.cdx.json"},
  "members": [
    {"purl": "pkg:cargo/api@0.1.0", "source_dir": "services/api"},
    {"purl": "pkg:npm/api@0.1.0", "source_dir": "services/api"}
  ]
}
```

`jq '.members | length'` returns `2`. Members sorted lex by `purl` (cargo < npm alphabetically).

## Schema URL

**Unchanged**: `SPLIT_MANIFEST_SCHEMA_V1` constant at `split_manifest.rs:14` stays `https://waybill.dev/schema/split-manifest/v1.json`.

**Rationale** (per Q1 clarification): additive-optional fields don't require URL bumps — every m215 consumer's JSON parser sees the new field as `null` / absent, ignores it, and continues to work. Only consumers who OPT INTO reading `.members` see the new shape.

## Deserialize contract

m215 payloads (no `members` field) round-trip through m219 `SplitEntry` with zero data loss:

```rust
let m215_json = r#"{"subproject_id": "libsafe.cargo", ..., "files": {"cyclonedx-json": "libsafe.cargo.cdx.json"}}"#;
let entry: SplitEntry = serde_json::from_str(m215_json).unwrap();
assert!(entry.members.is_none());  // #[serde(default)] fills in None

let re_serialized = serde_json::to_string(&entry).unwrap();
assert_eq!(re_serialized, m215_json);  // byte-identical round-trip
```

m219 multi-member payloads round-trip losslessly through m219 `SplitEntry`:

```rust
let m219_json = r#"{"subproject_id": "services-api.multi", ..., "members": [{"purl": "pkg:cargo/api@0.1.0", "source_dir": "services/api"}]}"#;
let entry: SplitEntry = serde_json::from_str(m219_json).unwrap();
assert_eq!(entry.members.as_ref().unwrap().len(), 1);
// Wait: emit-time invariant forbids Some(vec![1-element]) — a deserialize-time
// 1-element members[] is only reachable via a hand-crafted (invalid per m219) payload.
// The invariant is emit-side; deserialize is permissive.
```

## Consumer parsing guidance

Consumers reading `split-manifest.json`:

- **Legacy m215-era consumers**: no code change needed. `.members` doesn't exist → they don't read it → they see per-entry `{subproject_id, root_purl, source_dir, component_count, shared_deps_count, files}` as before.
- **m219-aware consumers**: check `if entry.get("members").is_some()` to detect multi-member groups. When present, iterate `members[]` to enumerate contributing main-modules.

## Test coverage matrix

Unit tests in `waybill-cli/src/generate/split_manifest.rs::tests` (extending the m215 test set):

| Test scenario | Assertion |
|---|---|
| Round-trip `SplitEntry` with `members: None` | Serialized JSON has NO `"members"` key |
| Round-trip `SplitEntry` with `members: Some(vec)` | Serialized JSON has `"members"` array; deserialize preserves order |
| m215 payload (no `members`) deserializes cleanly | `entry.members.is_none()` |
| m219 multi-member payload → m215 shape (jq strip `members`) is byte-identical to the m215 wire shape | Verifies additive-only nature |
| Sort order: `members` sorted lex by `purl` | Emit-time invariant tested via a synthetic populate |

## Failure modes

- **Deserialize with malformed `members`**: e.g., `"members": "not an array"` → `serde_json::Error`. Non-crashing; caller handles the error.
- **Deserialize with 1-element `members`**: valid deserialize (no invariant enforcement on the read side). Consumers seeing this can treat it as a m219-emitter bug and warn — but the m219 emitter never produces this shape (invariant at populate time).

## Backward compat guarantees

- Every existing m215 golden `.json` payload round-trips through m219 `SplitEntry` with byte-identical re-serialization.
- Every existing m215 test that reads `split-manifest.json` passes unchanged.
- SC-005 gate: single-member groups produce byte-identical output.
