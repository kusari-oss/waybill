# Annotation Wire Contracts: Milestone 161

**Date**: 2026-07-04
**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md) | **Data Model**: [data-model.md](../data-model.md)

Per-format wire shape for C112 (document-scope, single annotation). Uses raw string values (no envelope JSON), matching the milestone-158 (C104/C105) + milestone-159 (C106/C107) + milestone-160 (C108–C111) precedent.

## C112 — `mikebom:go-workspace-mode` (document-scope)

### CycloneDX 1.6

```json
{
  "metadata": {
    "properties": [
      {"name": "mikebom:go-workspace-mode", "value": "detected: 47 use-modules"}
    ]
  }
}
```

### SPDX 2.3

```json
{
  "annotations": [
    {
      "annotationDate": "2026-07-04T00:00:00Z",
      "annotationType": "OTHER",
      "annotator": "Tool: mikebom",
      "comment": "mikebom:go-workspace-mode=detected: 47 use-modules"
    }
  ]
}
```

Placed at document scope (not attached to any specific package's `annotations` array).

### SPDX 3.0.1

```json
{
  "type": "Annotation",
  "creationInfo": "_:CreationInfo-mikebom-scan",
  "annotationType": "other",
  "statement": "mikebom:go-workspace-mode=detected: 47 use-modules",
  "subject": "spdx:SpdxDocument"
}
```

**Value grammar** (per data-model.md W1):

```text
value          ::= detected | absent | malformed
detected       ::= "detected: " use_count " use-modules"
use_count      ::= <non-negative integer>          # Q2: 0 is legal
absent         ::= "absent"                        # rarely emitted; see emission rule
malformed      ::= "malformed: " reason
reason         ::= <closed-vocab-6-code identifier> [": " detail]
                 | "io-error: " detail
```

**Reason vocabulary** (closed-but-extensible per milestone-158 C105 governance):

| Reason code | Meaning |
|-------------|---------|
| `missing-use-close-paren` | Block-form `use ( ... )` missing terminating `)` |
| `invalid-use-path` | Empty or malformed path token in a `use` directive |
| `duplicate-use-path` | Same path appears in two `use` directives |
| `unknown-directive` | Unrecognized top-level token (not `go`, `use`, `replace`, comment) |
| `invalid-replace-syntax` | `replace` directive malformed |
| `io-error` | Filesystem error during parse (typically transient) |

**Universality**: MUST appear iff `WorkspaceMode` is `Detected` or `Malformed`. MUST NOT appear when `WorkspaceMode::Absent` (byte-identity guard per SC-003 — non-workspace scans remain byte-identical to pre-161).

## Parity catalog integration

Single row using the milestone-127 macro pattern at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`:

```rust
// cdx.rs
cdx_anno!(c112_cdx, "mikebom:go-workspace-mode", document);

// spdx2.rs
spdx23_anno!(c112_spdx23, "mikebom:go-workspace-mode", document);

// spdx3.rs
spdx3_anno!(c112_spdx3, "mikebom:go-workspace-mode", document);
```

Registration in `mikebom-cli/src/parity/extractors/mod.rs` (adjacent to the C110/C111 block):

```rust
ParityExtractor { row_id: "C112", label: "mikebom:go-workspace-mode", cdx: c112_cdx, spdx23: c112_spdx23, spdx3: c112_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

## Consumer jq recipes

```bash
# Detect workspace-mode SBOMs (CDX)
jq '.metadata.properties // []
    | map(select(.name == "mikebom:go-workspace-mode"))
    | .[0].value // "absent"' sbom.cdx.json

# Extract use-module count from CDX
jq -r '.metadata.properties[]
       | select(.name == "mikebom:go-workspace-mode")
       | .value
       | capture("detected: (?<n>[0-9]+) use-modules")
       | .n' sbom.cdx.json

# CI gate: fail if workspace-mode SBOM has malformed go.work
value=$(jq -r '.metadata.properties[] | select(.name == "mikebom:go-workspace-mode") | .value' sbom.cdx.json)
if [[ "$value" == malformed:* ]]; then
    echo "SBOM was generated from a malformed go.work file: $value"
    exit 1
fi
```

## Byte-identity guarantee (SC-003)

For non-workspace SBOMs (scans where `<rootfs>/go.work` does NOT exist):

- C112 MUST NOT appear at document scope.

This is the guard that keeps SC-003 achievable: the single-module milestone-090 `golang` fixture + the 10 non-Go fixtures × 3 formats = 33 goldens remain byte-identical to pre-161.

Post-161, a new `golang-workspace` fixture will be added to the milestone-090 fixture-cache repo (per Assumptions §6) for exercising the SC-010 integration test. Its goldens will carry the new C112 annotation.
