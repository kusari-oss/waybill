# Contract — `mikebom:file-paths` value shape

Phase 1 output. Defines the wire-format contract for the `mikebom:file-paths` annotation across CDX 1.6, SPDX 2.3, and SPDX 3 outputs.

## Pre-145 (broken)

The `extra_annotations` BTreeMap stores the value as `Value::String`:

```rust
// file_tier/mod.rs:232-234
if let Ok(file_paths_json) = serde_json::to_string(&paths_str) {
    extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(file_paths_json));
}
```

Result: every emitter sees `Value::String("[\"usr/sbin/losetup\"]")`.

Wire output:
- **CDX**: `{"name": "mikebom:file-paths", "value": "[\"usr/sbin/losetup\"]"}` (correct — CDX `properties[].value` is spec-typed `xs:string`, so the stringified form IS the canonical CDX shape)
- **SPDX 2.3**: envelope contains `"value": "[\"usr/sbin/losetup\"]"` (BROKEN — envelope value is free-form JSON, the stringified form is non-canonical)
- **SPDX 3**: envelope contains `"value": "[\"usr/sbin/losetup\"]"` (BROKEN — same reason)

## Post-145 (correct)

The `extra_annotations` BTreeMap stores the value as `Value::Array`:

```rust
// file_tier/mod.rs:232-234 (post-fix)
extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(paths_str));
```

Result: every emitter sees `Value::Array([String("usr/sbin/losetup"), ...])`.

Wire output:
- **CDX**: `{"name": "mikebom:file-paths", "value": "[\"usr/sbin/losetup\"]"}` (UNCHANGED — CDX's emitter at `cyclonedx/builder.rs:1086-1098` still stringifies non-String `Value`s via the milestone-144 envelope-coercion fix, so CDX wire bytes are byte-identical pre/post 145)
- **SPDX 2.3**: envelope contains `"value": ["usr/sbin/losetup"]` (FIXED — native array inside the envelope)
- **SPDX 3**: envelope contains `"value": ["usr/sbin/losetup"]` (FIXED — native array inside the envelope)

## Invariants

| | CDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| Wire-output type | `xs:string` (stringified) | native JSON array | native JSON array |
| Sort order | ascending | ascending | ascending |
| Cap | `FILE_PATHS_CAP` entries | same | same |
| Truncation flag | `mikebom:file-paths-truncated = "true"` | same | same |
| Empty list | omit the annotation entirely | same | same |

## Wire byte-deltas vs pre-145

| Format | File changed? |
|---|---|
| CDX | NO — byte-identical to pre-145 (the milestone-144 envelope-coercion handles it) |
| SPDX 2.3 | YES — envelope `"value"` shifts from quoted-string-of-array to native array |
| SPDX 3 | YES — same as SPDX 2.3 |

## Test contract (in-file unit test, post-fix)

```rust
#[test]
fn mikebom_file_paths_is_native_array_not_stringified() {
    let dir = tempfile::tempdir().unwrap();
    // Construct a synthetic file-tier component with one path.
    let entry = OrphanEntry::synthetic("usr/sbin/losetup", "deadbeef".repeat(8));
    let resolved = entry.into_resolved_component();
    let value = resolved
        .extra_annotations
        .get("mikebom:file-paths")
        .expect("mikebom:file-paths is present");
    // The KEY assertion:
    assert!(
        value.is_array(),
        "expected Value::Array, got {value:?}"
    );
    // Plus the existing FR-007 sort + cap invariants (preserved unchanged).
    let arr = value.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].as_str(), Some("usr/sbin/losetup"));
}
```

## Migration guidance for consumers

Consumers that currently parse `mikebom:file-paths` from SPDX 2.3 or SPDX 3 envelopes via `serde_json::from_str(value)` (treating value as a JSON-string-encoded array) MUST update to treat the value as a native JSON array. The expected transition:

```javascript
// BEFORE
const paths = JSON.parse(annotation.value);  // parse value as JSON string → array

// AFTER
const paths = annotation.value;              // value IS the array, directly
```

CDX consumers are unaffected (CDX wire shape unchanged).
