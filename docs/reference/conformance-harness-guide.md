# Conformance harness author guide for mikebom SBOMs

**Audience**: maintainers of external SBOM-conformance test suites that compare mikebom's CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1 emissions and want their harness to (a) read the same data the producer wrote, (b) correctly recognize legitimate format-spec asymmetries instead of flagging them as bugs.

**Why this guide exists**: mikebom is the only multi-format SBOM emitter in the conformance ecosystem. A naive harness that compares the three formats by grep-style identical-key matching will produce a flood of false-positive cross-format-inequivalence findings. The reality is that each format has different mechanisms for carrying the same data; mikebom uses each format's idiomatic mechanism, with a small number of intentional, format-spec-driven asymmetries. This document tells you exactly how to read each format and what asymmetries to expect.

**Status**: written 2026-05-04 against mikebom v0.1.0-alpha.13. Reflects the post-milestone-071 catalog state.

---

## Section 1 — How mikebom carries `mikebom:*` metadata in each format

mikebom emits a per-component metadata layer using namespaced keys of the form `mikebom:<name>` (e.g., `mikebom:source-files`, `mikebom:sbom-tier`). The carrier shape differs per format:

### CDX 1.6

Every `mikebom:*` key appears as a `properties[]` entry on a `components[]` element (or on `metadata.component` for the main-module).

```json
{
  "components": [{
    "name": "test", "purl": "pkg:generic/test@1.0.0",
    "properties": [
      { "name": "mikebom:source-files", "value": "go.sum, go.mod" },
      { "name": "mikebom:sbom-tier",    "value": "source" }
    ]
  }]
}
```

**To read**: walk `components[].properties[]`, filter by `name == "mikebom:<key>"`, collect `value`.

**Quirk**: CDX 1.6 schema requires `properties[].value` to be a **string only** — no arrays, objects, or non-string scalars. Where mikebom needs to carry a list, the value is encoded as a delimiter-joined string. The delimiter convention varies by key:

| Key | Delimiter |
|---|---|
| `mikebom:source-files` | `, ` (comma-space) |
| `mikebom:cpe-candidates` | ` \| ` (pipe with surrounding spaces) |
| `mikebom:source-connection-ids` | `,` (comma, no space) |

Pre-comparison normalization in your harness: split CDX `value` on the appropriate delimiter to recover the per-element set.

### SPDX 2.3

Every `mikebom:*` key appears as one entry in a `Package.annotations[]` array, wrapped in a JSON-string envelope inside the `comment` field. The envelope schema is **`mikebom-annotation/v1`**:

```json
{
  "packages": [{
    "name": "test",
    "SPDXID": "SPDXRef-test-pkg",
    "annotations": [{
      "annotator": "Tool: mikebom-0.1.0-alpha.13",
      "annotationDate": "1970-01-01T00:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-files\",\"value\":[\"go.sum\",\"go.mod\"]}"
    }]
  }]
}
```

**To read**:

1. For each `Package`, walk `annotations[]`.
2. For each annotation, parse `comment` as JSON (it's a JSON-encoded string).
3. Verify `parsed.schema == "mikebom-annotation/v1"`.
4. Match `parsed.field` against the key you're looking for (e.g., `"mikebom:source-files"`).
5. Extract `parsed.value`. Unlike CDX, this can be any JSON shape — string, array, object, etc.

The canonical envelope schema lives in mikebom source at `mikebom-cli/src/generate/spdx/contracts/mikebom-annotation.schema.json` and is round-trip-tested by `annotation_envelope_schema_matches_json_file`.

**Common harness mistake**: greping the document for the literal `"mikebom:source-files"` will match (the field name is in the JSON-encoded `comment` string), but you have to parse the `comment` to recover the actual `value`.

### SPDX 3.0.1

Every `mikebom:*` key appears as an `Annotation` element in `@graph[]` whose `subject` points at a `software_Package` element's `spdxId`, and whose `statement` field carries the SAME `mikebom-annotation/v1` envelope as SPDX 2.3:

```json
{
  "@context": "https://spdx.org/rdf/3.0.1/spdx-context.jsonld",
  "@graph": [
    { "type": "SpdxDocument", "spdxId": "https://example.org/spdx/doc" },
    { "type": "software_Package", "spdxId": "https://example.org/spdx/pkg",
      "name": "test", "software_packageUrl": "pkg:generic/test@1.0.0" },
    { "type": "Annotation",
      "subject": "https://example.org/spdx/pkg",
      "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-files\",\"value\":[\"go.sum\",\"go.mod\"]}" }
  ]
}
```

**To read**:

1. Walk `@graph[]` for elements with `type == "Annotation"`.
2. Decode the `statement` field as the same JSON envelope (`schema`, `field`, `value`).
3. **Important**: distinguish component-level vs. document-level annotations by the `subject` field. If `subject` points at the `SpdxDocument` element's IRI, it's a document-level annotation (e.g., `mikebom:trace-integrity-*`). Otherwise it's component-level (e.g., `mikebom:sbom-tier`).

mikebom's reference reader for this format is `extract_spdx3_annotation_values` at `mikebom-cli/src/parity/extractors/common.rs:147`.

### Document-level annotations

A small set of `mikebom:*` keys are **document-level**, not component-level:

- `mikebom:generation-context` (filesystem-scan / container-image-scan / build-time-trace)
- `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` (Go-specific, milestone 061)
- `mikebom:trace-integrity-events-dropped`
- `mikebom:trace-integrity-kprobe-attach-failures`
- `mikebom:trace-integrity-ring-buffer-overflows`
- `mikebom:trace-integrity-uprobe-attach-failures`
- `mikebom:os-release-missing-fields`

Their carriers are different per format:

| Format | Document-level carrier |
|---|---|
| CDX 1.6 | Top-level `metadata.properties[]` — same shape as component-level but on the document instead of a component. |
| SPDX 2.3 | Top-level `annotations[]` (sibling of `packages[]`, NOT inside any package). Same `mikebom-annotation/v1` envelope inside `comment`. |
| SPDX 3 | `Annotation` graph element whose `subject` IS the `SpdxDocument` element's `spdxId`. Same envelope inside `statement`. |

---

## Section 2 — Inherent format-spec asymmetries (do NOT flag these)

mikebom's parity catalog declares **7 rows** where cross-format equivalence is intentionally NOT byte-equal. Each is driven by an unavoidable format-spec divergence. A correctly-configured harness should suppress cross-format-inequivalence findings on these rows; flagging them produces noise.

mikebom uses a `Directionality` enum to classify what each row asserts:

- `SymmetricEqual` — the three sets MUST be byte-identical after canonicalization. Default for `mikebom:*` keys.
- `CdxSubsetOfSpdx` — `cdx_set ⊆ spdx23_set ∧ cdx_set ⊆ spdx3_set`. SPDX may carry richer detail.
- `PresenceOnly` — all three formats carry the datum but in shapes that legitimately differ. Assert non-empty in all three; do not assert value equality.
- `CdxOnly` — CDX is the only format expressing this signal because SPDX has a native standards-side construct that supersedes the `mikebom:*` annotation.

Source of truth: `mikebom-cli/src/parity/extractors/common.rs:41`.

### The 7 non-`SymmetricEqual` rows

| Row | Key / label | Directionality | Reason | Standards-native superseding construct |
|---|---|---|---|---|
| **A12** | CPE | `CdxSubsetOfSpdx` | CDX `metadata.component.cpe` / `components[].cpe` is single-valued; SPDX 2.3 + 3 carry the full candidate set as `externalRefs[].referenceLocator` (one `cpe23Type` row per candidate). | CDX `cpe` field is the primary; SPDX `externalRefs[]` is the union. CDX values must be a subset of SPDX values; not the other way around. |
| **B4** | image / filesystem root | `PresenceOnly` | Each format encodes the scan subject in its own native primary-component construct (CDX `metadata.component`, SPDX `documentDescribes` / SPDX 3 `rootElement`). The shapes differ; the underlying datum is the same scan target. | Native BOM-subject slot per format. |
| **C19** | `mikebom:cpe-candidates` | `PresenceOnly` | CDX delivers candidates as a ` \| `-pipe-joined string (CDX 1.6 schema mandates `properties[].value` is a string); SPDX 2.3 + 3 deliver them as a JSON array inside the envelope. mikebom's own extractor splits the CDX side on `" \| "` for set-equality comparison, but the WIRE bytes legitimately differ in escape conventions: PURL slashes inside CPEs are single-backslash-escaped in CDX (`github.com\/foo`) and double-backslash-escaped in SPDX-envelope wire form (`github.com\\\\/foo`). The atomic CPE values are semantically equal; the cosmetic escape conventions differ. | A12's `cpe` field carries the highest-signal candidate per Constitution Principle V. C19 is the supplementary candidate set. |
| **C22** | `mikebom:os-release-missing-fields` | `PresenceOnly` | CDX uses comma-joined-string-with-trailing-empty shape when the input list is empty; SPDX uses a real JSON-array-valued envelope. The atomic atoms differ — CDX cannot losslessly emit a JSON array via a property's `value` (CDX 1.6 `properties[].value` is stringly-typed). Both formats carry the same set of missing-field names; the shape divergence is format-mandated. | None — this is a mikebom-specific advisory annotation; CDX can't natively express the array shape. |
| **C42** | `mikebom:lifecycle-scope` | `CdxOnly` | CDX `scope: "excluded"` cannot express the dev/build/test sub-distinction the milestone-052 work needed; mikebom emits `mikebom:lifecycle-scope` on CDX components for the finer split. SPDX 2.3 + 3 carry this signal natively via dedicated dep-relationship types (`DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` in SPDX 2.3; `LifecycleScopeType` parameter in SPDX 3) — asserted independently by row B2's typed-edge extractor. | SPDX 2.3 native dep-relationship types; SPDX 3 `lifecycleScope` parameter. Constitution Principle V's named motivating case. |
| **D1** | evidence — identity | `PresenceOnly` | CDX nests under `evidence.identity[]` as an array of `{technique, confidence}` objects per component; SPDX 2.3 + 3 emit a flat `mikebom:evidence-kind` + `mikebom:confidence` annotation pair. The shapes are structurally distinct; the underlying datum (technique + confidence float) is the same. | None — both shapes are non-spec-native; the divergence is mikebom's choice driven by what each spec's evidence model supports. |
| **E1** | ecosystem completeness | `PresenceOnly` | CDX uses a `compositions[]` array where each entry can be flagged `complete`/`incomplete`; SPDX 2.3 + 3 emit a single `mikebom:complete-ecosystems: [<name>, ...]` annotation listing the ecosystems mikebom claims complete coverage of. CDX-array shape vs. SPDX-list shape; same underlying claim. | CDX `compositions[]` is the format-native construct. |

### The 1 row with format-restricted classification

| Row | Key / label | Notes |
|---|---|---|
| (catalog A5) | author | Catalog row exists but no extractor defined yet. Awaits emit-side wiring. Harnesses will see all three formats empty for this row. |

---

## Section 3 — How mikebom verifies parity internally (use this as your harness reference)

The authoritative mikebom-internal cross-format-parity assertion lives at `mikebom-cli/tests/holistic_parity.rs`. It runs on every `cargo test --workspace` and is the canonical source of truth. Its logic, distilled:

```rust
for row in catalog_rows {
    if !row.is_universal_parity() { continue }   // skip Restricted rows
    let cdx_set    = (extractor.cdx)(&cdx_doc);
    let spdx23_set = (extractor.spdx23)(&spdx23_doc);
    let spdx3_set  = (extractor.spdx3)(&spdx3_doc);
    match extractor.directional {
        SymmetricEqual    => assert!(cdx_set == spdx23_set && spdx23_set == spdx3_set),
        CdxSubsetOfSpdx   => assert!(cdx_set.is_subset(&spdx23_set)
                                  && cdx_set.is_subset(&spdx3_set)),
        PresenceOnly      => if any_present { assert!(all_present) },
        CdxOnly           => { /* SPDX sides not asserted */ }
    }
}
```

Each per-format extractor returns a `BTreeSet<String>` of canonicalized atomic values. The canonicalization layer (`canonicalize_atomic_values` at `extractors/common.rs:213`) handles the format-shape differences described in Section 1 — string-encoded JSON values are recursively decoded, arrays are flattened. Two semantically-equivalent values that differ only in encoding produce the same canonical string.

**Recommendation for harness authors**: replicate this `Directionality`-aware check rather than doing flat byte-grep cross-format equality. The current published catalog rows are at `docs/reference/sbom-format-mapping.md` (the catalog table); the directionality flags are in mikebom source at `parity/extractors/mod.rs`.

---

## Section 4 — Things to expect that aren't bugs (false-positive prevention)

A harness that does naive top-level grep across the three formats will hit the following non-bugs. List them in your harness's allowlist.

### 4.1 SPDX 2.3 has no top-level `mikebom:*` properties

A grep for `properties[].name == "mikebom:source-files"` in SPDX 2.3 will return zero hits — because SPDX 2.3 has no `properties[]` in the first place. The data is inside `Package.annotations[].comment` as a JSON-encoded envelope. **Decode the envelope before declaring "missing".**

### 4.2 SPDX 3 has no top-level `mikebom:*` properties either

Same shape: SPDX 3 carries the data inside `Annotation` graph elements via the same envelope. The `Package` element has no direct `mikebom:*` field.

### 4.3 CDX delivers some lists as delimited strings; SPDX delivers them as arrays

`mikebom:source-files` and `mikebom:cpe-candidates` are common examples. Pre-comparison split the CDX string on the appropriate delimiter (per the table in §1).

### 4.4 PURL escape conventions differ between CDX-property strings and SPDX-envelope strings

For keys whose values contain PURLs (especially `mikebom:cpe-candidates`), the WIRE BYTES of slashes legitimately differ:

- CDX (single-backslash, JSON-string-encoded): `cpe:2.3:a:github.com\/foo:foo:1.0:*:*:*:*:*:*:*`
- SPDX 2.3 / 3 (double-backslash inside the envelope's nested JSON-string): `cpe:2.3:a:github.com\\/foo:foo:1.0:*:*:*:*:*:*:*`

Both decode to the same atomic CPE: `cpe:2.3:a:github.com/foo:foo:1.0:*:*:*:*:*:*:*`. **Decode both before byte-comparison**, or accept the row as `PresenceOnly` per the catalog directionality.

### 4.5 mikebom's CDX `cpe` field is single-valued; SPDX 2.3 + 3 list every candidate

This is the A12 `CdxSubsetOfSpdx` case: CDX's `metadata.component.cpe` (and per-component `cpe`) carries one CPE — the highest-signal candidate. SPDX 2.3 + 3 emit one `externalRef.referenceType: "cpe23Type"` row per candidate. Cardinality differs by design. Treat the CDX value as a member of the SPDX set, not as the full set.

### 4.6 `mikebom:lifecycle-scope` is intentionally CDX-only

If your harness sees this annotation in CDX components but never in SPDX, that is correct. SPDX's lifecycle scope is carried natively via `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` relationships (SPDX 2.3) and `lifecycleScope` parameters (SPDX 3) — different keys, same signal.

### 4.7 `compositions` and `evidence` differ structurally between CDX and SPDX

CDX has dedicated `compositions[]` and `evidence.identity[]` constructs; SPDX uses a flat annotation pair. Same datum, structurally different shape. Treat as `PresenceOnly` per the catalog.

### 4.8 Document-level vs component-level placement

Keys like `mikebom:generation-context` and `mikebom:trace-integrity-*` live at the **document level** in all three formats — but the carrier slot differs (`metadata.properties[]` in CDX, top-level `annotations[]` in SPDX 2.3, `Annotation.subject = SpdxDocument-IRI` in SPDX 3). Don't look for them in component-level slots.

---

## Section 5 — mikebom-specific quirks and known weaknesses

This is the section to track changes mikebom may need to make. As of milestone 071:

### 5.1 ✅ FIXED in milestone 071: `mikebom sbom parity-check` now does real value-equality checking

**Pre-071 behavior**: the CLI subcommand `mikebom sbom parity-check` reported "0 parity gaps" whenever all three formats had `≥1 entry` per catalog row, regardless of whether the actual set CONTENTS matched. A row where CDX had `["go.sum"]` and SPDX 2.3 had `["DRIFTED.sum"]` would report "✓" — both were non-empty, the presence-only check accepted it.

**Post-071 behavior**: the subcommand now applies the real `Directionality` invariants — set equality for `SymmetricEqual`, ⊆ for `CdxSubsetOfSpdx`, presence-parity for `PresenceOnly`, CDX-non-empty for `CdxOnly`. A real value drift now produces `Parity gaps: 1` and the per-row diff is shown in JSON output.

**Harness implication**: if your harness shells out to `mikebom sbom parity-check` and trusts its `0 parity gaps` line, your harness was missing real bugs pre-071. Upgrade to mikebom v0.1.0-alpha.14 or later for the rigorous check.

A regression test pinning this behavior lives at `mikebom-cli/tests/parity_synthetic_drift.rs` — it builds a synthesized drift triple and asserts that pre-071 logic would have missed the gap while post-071 logic catches it.

### 5.2 Delimiter conventions in CDX property values are inconsistent

Pre-071 mikebom emits:

- `mikebom:source-files` — `, ` (comma-space)
- `mikebom:cpe-candidates` — ` | ` (pipe-with-spaces)
- `mikebom:source-connection-ids` — `,` (comma-no-space)

Three different delimiters for three different list-valued keys. This is a wart; a future mikebom milestone may unify these. Until then, harnesses must hardcode the per-key delimiter.

### 5.3 Catalog rows declared but not emitted

The mikebom catalog declares 18 rows for keys whose emit code paths exist but only fire on specific scan inputs (e.g., `mikebom:elf-build-id` only when an ELF binary is scanned; `mikebom:macho-uuid` only on Mach-O; `mikebom:pe-machine` only on Windows PE). Harnesses running on input that doesn't exercise these paths will see all three formats empty for these rows. That is **not a parity gap** — it's an unexercised row. The post-071 `mikebom sbom parity-check` output correctly counts these as "passing by default" rather than penalizing them.

### 5.4 The `MikebomAnnotationCommentV1` envelope is V1

If mikebom ever needs to extend the envelope shape (e.g., add a `confidence_attribution` field), the schema field will become `mikebom-annotation/v2` and the V1 readers must be kept working in parallel. Harness authors should treat unknown `schema` values as "ignore" rather than "error" to avoid breaking on a future schema bump.

### 5.5 No JSON-LD context for SPDX 3 mikebom annotations

SPDX 3.0.1 is JSON-LD; ideally `mikebom:*` keys would be IRIs in a registered context document. Today they're plain string field names inside the envelope. This means SPDX 3 readers that expect IRI-typed annotations may need a custom decode step to recognize the envelope. Future milestone candidate: register a `mikebom` namespace in a JSON-LD context document and use IRIs.

### 5.6 Same-PURL collision dedup may surface different metadata across formats

When two scan paths discover the same PURL (e.g., a workspace member appearing in both Cargo.lock and the workspace Cargo.toml main-module pass), mikebom dedups to one canonical entry per format. The dedup *output* should be identical across formats — verified by `holistic_parity.rs` — but if you see a SymmetricEqual gap on a duplicated PURL, file an issue. (Tracked separately as #125 for the divergent-PURL case.)

---

## Section 6 — Recommended harness wiring

For a harness that wants to consume mikebom SBOMs cleanly:

### 6.1 Implement the `MikebomAnnotationCommentV1` envelope decoder

Reference Rust implementation: `extract_mikebom_annotation_values` at `mikebom-cli/src/parity/extractors/common.rs:96`. The decoder walks the appropriate annotation pool (component-level vs. document-level), parses each `comment` as JSON, matches `field` against the target key, and extracts `value`.

A Python equivalent might be:

```python
def decode_mikebom_envelope(comment_str, target_field):
    try:
        env = json.loads(comment_str)
    except (json.JSONDecodeError, TypeError):
        return None
    if env.get("schema") != "mikebom-annotation/v1":
        return None
    if env.get("field") != target_field:
        return None
    return env.get("value")
```

### 6.2 Apply per-Directionality invariants, not flat equality

Read the catalog (`docs/reference/sbom-format-mapping.md`'s parity table or via mikebom source) for each row's directionality and apply the right check. Treating every row as `SymmetricEqual` produces noise on the 7 rows in §2.

### 6.3 Use `mikebom sbom parity-check --format json` for machine consumption

Post-071 the JSON output format is the rigorous source of truth:

```bash
mikebom sbom parity-check --scan-dir <dir> --format json | jq '.summary.parity_gaps'
```

`0` means clean. Any positive number means real cross-format drift; the per-row breakdown in `.rows[]` shows which rows failed and what the per-format sets contained.

### 6.4 Skip document-level rows from component-level harness checks

The 7 document-level keys named in §1 should be checked at the document level only. A harness doing per-component CFI will incorrectly flag them as missing when scanning components.

---

## Section 7 — Where to read the canonical specs

- **Catalog table** (per-row metadata, including directionality): `docs/reference/sbom-format-mapping.md` "Cross-format datum × per-format mapping" section.
- **Envelope schema**: `mikebom-cli/src/generate/spdx/contracts/mikebom-annotation.schema.json` (JSON Schema).
- **Directionality enum** (Rust): `mikebom-cli/src/parity/extractors/common.rs:41`.
- **Holistic parity assertion** (Rust integration test, the canonical assertion): `mikebom-cli/tests/holistic_parity.rs`.
- **Synthetic drift regression test**: `mikebom-cli/tests/parity_synthetic_drift.rs`.
- **CLI subcommand** (post-071, value-equality): `mikebom sbom parity-check --scan-dir <dir>`.

If anything in this guide is unclear or appears inconsistent with the source, the source wins — please file an issue against the mikebom repo with the specific ambiguity.
