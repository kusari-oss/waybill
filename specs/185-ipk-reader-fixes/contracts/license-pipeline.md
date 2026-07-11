# Contract: opkg License 4-Pass Normalization Pipeline (m185 US2)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Scope

Canonical pipeline contract for extracting and normalizing the opkg `License:` field into a `PackageDbEntry.licenses: Vec<SpdxExpression>` value. Applies to `mikebom-cli/src/scan_fs/package_db/opkg.rs::build_entry` at line 203. Fires once per stanza during `opkg::read`.

## Reused helpers (rpm_file.rs, pub(crate) after Decision 2 + 4)

- `super::rpm_file::normalize_bitbake_license_operators(raw: &str) -> String` — Pass 1
- `super::rpm_file::preserve_known_operands_with_license_ref(raw: &str) -> Option<String>` — Pass 3
- `super::rpm_file::sanitize_to_license_ref_idstring(raw: &str) -> Option<String>` — Pass 4 helper

## Pipeline shape

```
stanza.license()  →  Option<&str>
     │
     ├── None or whitespace-only  →  Vec::new() (FR-007 absent-License regression pin)
     │
     ▼ Some(raw non-whitespace)
   Pass 1: normalize_bitbake_license_operators(raw)  →  String
     │  Substitutes ` & ` → ` AND `, ` | ` → ` OR ` (space-delimited operator normalization).
     │  Idempotent: no-op on already-canonical input.
     ▼
   Pass 2: SpdxExpression::try_canonical(&normalized)
     │  Strict SPDX 2.3 parse; succeeds on already-canonical or fully-recognized expressions.
     │
     ├── Ok(expr)  →  vec![expr]  (DONE — happy path)
     │
     ▼ Err
   Pass 3: preserve_known_operands_with_license_ref(&normalized)  →  Option<String>
     │  Per-operand LicenseRef wrap: unrecognized operands become
     │  `LicenseRef-<sanitized>`, recognized operands pass through unchanged.
     │  Then re-run SpdxExpression::try_canonical(&wrapped).
     │
     ├── Some(wrapped) + try_canonical(&wrapped) == Ok(expr)  →  vec![expr]  (DONE — #481 escape hatch)
     │
     ▼ None OR try_canonical(&wrapped) == Err
   Pass 4 (m185 US2 wholesale-wrap, opkg-only per Decision 3):
     │  sanitize_to_license_ref_idstring(raw)  →  Option<String>
     │  If Some(sanitized): wrapped = format!("LicenseRef-{sanitized}")
     │  SpdxExpression::try_canonical(&wrapped)
     │
     ├── None (unsanitizable — no idstring-safe chars in raw)  →  Vec::new() (fail-safe)
     │
     ├── Ok(expr)  →  vec![expr] (m185 wholesale-wrap fallback)
     │       │
     │       └── tracing::warn! logged with raw + wrapped strings for
     │           operator visibility.
     │
     └── Err (extremely unlikely — sanitized string produces invalid SPDX)
              →  Vec::new() (defensive fail-safe)
```

## Pass-by-pass invariants

### Pass 1 — Operator normalization

- **Input**: raw String from `stanza.license()`.
- **Contract**: `normalize_bitbake_license_operators` at `rpm_file.rs:615` — space-delimited operator substitution. Idempotent.
- **Failure mode**: none (function always returns a String).

### Pass 2 — Strict SPDX parse

- **Input**: operator-normalized String.
- **Contract**: `SpdxExpression::try_canonical` from `mikebom_common::types::license` — strict SPDX 2.3 grammar.
- **Success signal**: `Ok(SpdxExpression)` returned. Terminates the pipeline.
- **Failure signal**: `Err`. Advance to Pass 3.

### Pass 3 — Per-operand LicenseRef wrap (rpm #481 escape hatch)

- **Input**: same operator-normalized String from Pass 1.
- **Contract**: `preserve_known_operands_with_license_ref` at `rpm_file.rs:832` — tokenize + per-operand classify + rebuild.
- **Success signal**: `Some(wrapped)` returned AND `try_canonical(&wrapped) == Ok(expr)`. Terminates the pipeline.
- **Failure signals**:
  - `None` returned — the input can't be tokenized into a valid SPDX structure (broken parens, dangling operators, etc.). Advance to Pass 4.
  - `Some(wrapped)` but `try_canonical(&wrapped) == Err` — the wrapped result STILL doesn't canonicalize (very rare). Advance to Pass 4.

### Pass 4 — Wholesale-wrap fallback (m185 US2, opkg-only)

- **Input**: original raw String (NOT the operator-normalized string — we want to preserve the exact operator content in the LicenseRef).
- **Contract**:
  1. `sanitize_to_license_ref_idstring(raw)` — sanitize per SPDX 2.3 §10 idstring grammar (replace non-`[A-Za-z0-9.-]` chars with `-`).
     - Returns `None` when the raw string contains NO idstring-safe chars (empty result).
  2. Prefix with `LicenseRef-` → wrapped string.
  3. `try_canonical(&wrapped)` — validate the wrapped LicenseRef is well-formed SPDX.
  4. Emit `tracing::warn!` with the raw + wrapped values (operator visibility).
- **Success signal**: `Some(SpdxExpression)` — the wrapped LicenseRef canonicalizes. Terminates the pipeline.
- **Failure signals**:
  - `sanitize_to_license_ref_idstring` returns `None` — the raw string had no salvageable content. Emit `licenses: Vec::new()` (fail-safe, matches FR-007 shape).
  - `try_canonical(&wrapped) == Err` — extremely rare defensive-guard path. Emit `licenses: Vec::new()`.

## Behavior invariants (cross-pass)

1. **Whitespace-only input treated as absent** — same as `None`. Pipeline never advances past the filter guard.
2. **Fully canonical input**: Pass 2 succeeds; Passes 3 and 4 never fire. Byte-identical to what rpm emits for the same input.
3. **Bit-Bake operator input** (`GPLv2 & MIT`): Pass 1 normalizes; Pass 2 succeeds via the normalized form. Byte-identical to rpm's behavior for the same input.
4. **Mixed known + unknown operands** (`GPLv2 & bzip2-1.0.4`): Pass 1 normalizes; Pass 2 fails; Pass 3 succeeds via per-operand LicenseRef wrap. Byte-identical to rpm's behavior for the same input.
5. **Wholly unparseable input** (`!!! broken &&& syntax !!!`): Passes 1–3 all fail; Pass 4 emits `LicenseRef-broken-syntax` (or similar sanitized form). This DIFFERS from rpm — rpm's pipeline emits `Vec::new()` in this case. m185 opkg-only extension per Decision 3.
6. **Fully unsanitizable input** (`!` or purely-symbol strings): Pass 4's `sanitize_to_license_ref_idstring` returns `None`; emitted `licenses: Vec::new()`. Fail-safe matches FR-007 absent-License shape.

## rpm-side non-modification invariant (SC-005, FR-011)

The rpm reader's call site (`rpm_file.rs:469-488`) is UNCHANGED. It continues to use only Passes 1–3 (its existing pipeline). The 4th pass exists ONLY on the opkg call site. This ensures:

- rpm goldens across CDX 1.6, SPDX 2.3, SPDX 3.0.1 stay byte-identical.
- Any rpm License string that pre-m185 produced `licenses: Vec::new()` continues to do so.
- The rpm-specific test suite (m152 + m165 + m168 audit-harness family) is unaffected.

## Emitted format shape (all three formats)

For `stanza.license() == Some("GPLv2 & bzip2-1.0.4")` (the canonical Yocto shape):

- **CDX 1.6** `components[].licenses`:
  ```json
  [
    { "license": { "acknowledgement": "declared", "id": "GPL-2.0-only" } },
    { "license": { "acknowledgement": "declared", "name": "LicenseRef-bzip2-1.0.4" } }
  ]
  ```
- **SPDX 2.3** package fields:
  ```json
  {
    "licenseDeclared": "GPL-2.0-only AND LicenseRef-bzip2-1.0.4",
    "hasExtractedLicensingInfos": [
      {
        "licenseId": "LicenseRef-bzip2-1.0.4",
        "extractedText": "bzip2-1.0.4"
      }
    ]
  }
  ```
- **SPDX 3.0.1** `elements[]`:
  ```json
  {
    "simplelicensing_CustomLicense": {
      "licenseText": "bzip2-1.0.4"
    }
  }
  ```

Emission shape ownership: the `mikebom-common::types::license::SpdxExpression` → format-specific transform layer already handles all three formats correctly (unchanged from pre-m185). m185 just populates the input `Vec<SpdxExpression>`.
