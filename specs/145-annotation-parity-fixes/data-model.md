# Data Model — milestone 145

Phase 1 output. Identifies the in-memory data shape changes and emission-shape changes for milestone 145.

## Shape changes (the three annotation values)

### `mikebom:file-paths` (US1)

| | Pre-145 | Post-145 |
|---|---|---|
| Storage type in `extra_annotations` BTreeMap | `Value::String("[\"path1\",\"path2\"]")` (JSON-string-encoded array) | `Value::Array([Value::String("path1"), Value::String("path2")])` (native array) |
| CDX wire shape | `{"name": "mikebom:file-paths", "value": "[\"path1\",\"path2\"]"}` (string literal) | `{"name": "mikebom:file-paths", "value": "[\"path1\",\"path2\"]"}` (CDX spec-types `properties[].value` as `xs:string`; the milestone-144 envelope-coercion stringifies the array on the CDX side — verify this is the post-fix behavior, NOT a regression) |
| SPDX 2.3 envelope value | `"value": "[\"path1\",\"path2\"]"` (string-of-array) | `"value": ["path1", "path2"]` (native array inside the envelope) |
| SPDX 3 envelope value | `"value": "[\"path1\",\"path2\"]"` (string-of-array) | `"value": ["path1", "path2"]` (native array inside the envelope) |

**Verification on CDX wire shape**: per the milestone-144 envelope-value coercion fix (`94e8434`), CDX's `property.value` is spec-typed as `xs:string` — the builder at `cyclonedx/builder.rs:1086-1098` stringifies non-String `Value`s via `serde_json::to_string(other)`. So CDX's wire output for `mikebom:file-paths` post-145 will be `"value": "[\"path1\",\"path2\"]"` — IDENTICAL to pre-145 wire bytes. This is correct behavior: the SPEC-LEVEL change is in the SPDX 2.3 + SPDX 3 envelope values (which carry native JSON types); CDX's wire shape doesn't change because the format itself doesn't carry richer types. The audit-flagged 3,112 findings are dominated by the SPDX 2.3 and SPDX 3 envelope comparisons. The harness-finding count reduction is real.

### `mikebom:lifecycle-scope` (US2)

| | Pre-145 | Post-145 |
|---|---|---|
| CDX emission (unchanged) | `{"name": "mikebom:lifecycle-scope", "value": "development"}` on non-Runtime components | unchanged |
| SPDX 2.3 emission (unchanged) | envelope with `field = "mikebom:lifecycle-scope", value = "development"` on non-Runtime components | unchanged |
| SPDX 3 emission | NOT EMITTED (cluster pattern `Y \| Y \| -`) | envelope with `field = "mikebom:lifecycle-scope", value = "development"` on non-Runtime components |

### `mikebom:source-files` (US3)

| | Pre-145 | Post-145 |
|---|---|---|
| `c.evidence.source_file_paths` field value | Rootfs-relative path list (from milestone-133 normalization) — e.g., `["root/.m2/repository/.../foo.jar"]` | unchanged |
| `c.extra_annotations["mikebom:source-files"]` value | Maven reader stamps `Value::String("<outer>!<inner>!...")` (JAR-URL notation) | Per US3 fix path: either (a) field-emission keeps winning + emitter-side guard suppresses the extra_annotations duplicate, OR (b) Maven reader stops stamping into this key (uses `mikebom:source-files-nested-url` or similar). Implement-phase decision based on fixture-reproduction. |
| All three emitter outputs | Diverge by emitter due to dedup-order difference | Single consistent value per component across CDX / SPDX 2.3 / SPDX 3 |

## New types / structs introduced

**None.** Milestone 145 is pure emission-shape correction; no new domain types, no new structs, no new enums. The existing types are sufficient:
- `serde_json::Value::Array` for US1 (already pervasive)
- `mikebom_common::resolution::LifecycleScope` for US2 (milestone-049, unchanged)
- `BTreeMap<String, serde_json::Value>` for `extra_annotations` (unchanged)

## Modified functions / sites

| File | Function / site | Change |
|---|---|---|
| `mikebom-cli/src/scan_fs/file_tier/mod.rs` (~line 233) | `into_resolved_component()`'s `mikebom:file-paths` insertion | `json!(file_paths_json)` → `json!(paths_str)` (drop the `to_string` round-trip) |
| `mikebom-cli/src/scan_fs/file_tier/mod.rs` (~line 405) | Existing unit test parse logic | `serde_json::from_str(fp).expect(...)` → `fp.as_array().expect("file-paths is array")...` |
| `mikebom-cli/src/generate/spdx/v3_annotations.rs` (around line 263) | Annotation-emission function | NEW branch: emit `mikebom:lifecycle-scope` for non-Runtime scopes mirroring `annotations.rs:227-236` |
| `mikebom-cli/src/scan_fs/package_db/maven.rs:2244` (US3, depending on fix choice) | Maven nested-JAR reader's `extra_annotations` stamping | Either: drop the `mikebom:source-files` key stamping entirely (option 2a), OR rename to a non-colliding key like `mikebom:source-files-nested-url` (option 2b) |
| `mikebom-cli/src/generate/cyclonedx/builder.rs:1086`, `spdx/annotations.rs:371`, `spdx/v3_annotations.rs:332` (US3, depending on fix choice) | `extra_annotations` iteration emission sites | Add a guard against re-emitting keys already emitted from field-derived sources (option 1) — currently `["mikebom:source-files"]` only; may extend. |

## Validation rules (consolidated from spec FRs)

| Input | Rule | Source |
|---|---|---|
| `mikebom:file-paths` value in `extra_annotations` | MUST be `Value::Array`, NEVER `Value::String` (post-145) | FR-001 + SC-002 |
| `mikebom:file-paths` array element ordering | MUST be sorted ascending (preserves FR-007 of milestone 133) | FR-002 |
| `mikebom:file-paths-truncated` sidecar | MUST still appear when `paths.len() > FILE_PATHS_CAP` | FR-002 |
| `mikebom:lifecycle-scope` SPDX 3 emission | MUST emit for `Development`/`Build`/`Test`; MUST omit for `Runtime` and `None` | FR-005 + FR-006 + FR-007 |
| `mikebom:source-files` cross-format value | MUST be byte-equivalent across CDX, SPDX 2.3, SPDX 3 for the same component on the same scan | FR-009 + SC-008 |
| `c.evidence.source_file_paths` field | Remains the canonical source for emission (unchanged from milestone 133's normalization pipeline) | research §C, Decision part 1 |

## Out of model

- New types (no new structs / enums / newtypes).
- Changes to the `ResolvedComponent` struct shape (only its field VALUES are recomputed; field LAYOUT unchanged).
- Changes to the parity-catalog row schema (`row_id`, `label`, `directional`, etc.) — C18 / C42 / C92 already exist with the correct `Directionality::SymmetricEqual`.
- New `mikebom:*` annotations (explicitly out-of-scope per spec FR-011).
