# Mutations (FR-007)

Each mutation is applied with an exact replacement, the named test is run, then the file is reverted with `cp` and checked with `cmp`. The test is `closure_runtime_scope::us4_drop_scope_excluded_yields_valid_runtime_document`, which carries the reference-removal conformance check after T003.

## #1725: what the moat `spdx3-validate` run guarded (T004, research R2)

All mutations are in `waybill-cli/src/edit/spdx3.rs` `remove_openvex_references`. Each leaves something on the `SpdxDocument` where the OpenVEX reference was removed.

| mutation | result | caught by |
|---|---|---|
| M1: keep the empty `externalRef: []` (drop the `is_empty` removal) | PASS | nothing; the empty array is conformant |
| M2: leave a locator-less OpenVEX `ExternalRef` | PASS | nothing; `locator` is optional, so it is conformant |
| M3: leave an `ExternalRef` with `externalRefType: "notAnSpdx3RefType"` | FAIL | JSON schema check (`closure_runtime_scope.rs:2349`) |
| M4: leave an OpenVEX `ExternalRef` with `contentType: "not a media type"` | FAIL | T003's "reference survived" assertion (`:2345`) |
| M5: the same with `contentType: 7` | FAIL | T003's "reference survived" assertion (`:2345`) |
| M6: leave an `ExternalRef` with `externalRefType: "other"` and `contentType: "not a media type"` | FAIL | JSON schema check (`:2349`), whose schema constrains the `contentType` pattern (`spdx-3.0.1.json:1076`) |
| Control: the same with a valid `contentType: "application/json"` | PASS | (the injection itself is conformant) |

**Finding:** on the reference-removal path, every non-conformance produced was caught by the JSON schema check, or by T003's new assertion, before `spdx3-validate` ran. No mutation was found that the schema passes and `spdx3-validate` rejects.

The removal only touches the `SpdxDocument`'s `externalRef[]`. That is within what the JSON schema describes (types, enums, patterns), not the cross-element relations SHACL adds.

So on this path, the moat validator run in i1454 guarded nothing that the schema check, which i1454 keeps, doesn't already catch. `us4` still runs `spdx3-validate` on its small edited document, which costs 1.8 s for the whole test.

## T003 control (test side)

Mutation C1: the input handed to the "carries the reference" control has its references stripped, standing in for a stub that stops emitting the sidecar.

```
        FAIL [   0.745s] (1/1) waybill::closure_runtime_scope us4_drop_scope_excluded_yields_valid_runtime_document
    assertion `left == right` failed: spdx23: the input must reference its OpenVEX sidecar, or nothing below removes one
```

The control fires, so `us4` cannot silently stop exercising the reference-removal path.
