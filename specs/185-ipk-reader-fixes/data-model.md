# Data Model: ipk reader bug fixes (m185)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. No New Types

m185 introduces ZERO new types. Both fixes operate on existing type surfaces:

- **`parse_ipk_filename`** (`ipk_file.rs:609`) — return type `Option<(String, String, String)>` unchanged.
- **`filename_fallback_entry`** (`ipk_file.rs:557`) — signature + return type unchanged (`Option<PackageDbEntry>`).
- **`opkg::build_entry`** (`opkg.rs:203`) — signature + return type unchanged. Only the `licenses` field initialization at line 289 changes.
- **`PackageDbEntry.licenses: Vec<SpdxExpression>`** — existing type, unchanged shape. m185 populates it for opkg entries that pre-m185 emitted with `Vec::new()`.

## 2. US1 — Parser Semantic Change

### 2.1 Function under change

**File**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`
**Function**: `fn parse_ipk_filename(filename: &str) -> Option<(String, String, String)>` at line 609

### 2.2 Current implementation (pre-m185)

```rust
fn parse_ipk_filename(filename: &str) -> Option<(String, String, String)> {
    let stem = filename.strip_suffix(".ipk")?;
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() != 3 {
        return None;  // Rejects multi-underscore versions
    }
    let name = parts[0].to_string();
    let version = parts[1].to_string();
    let arch = parts[2].to_string();
    if name.is_empty() || version.is_empty() || arch.is_empty() {
        return None;
    }
    Some((name, version, arch))
}
```

### 2.3 Post-m185 implementation (per Decision 1)

```rust
fn parse_ipk_filename(filename: &str) -> Option<(String, String, String)> {
    let stem = filename.strip_suffix(".ipk")?;
    // Milestone 185 US1 (#538) — right-to-left split so version fields
    // containing embedded underscores (BitBake SRCPV expansion for
    // git-sourced upstream builds) are correctly captured. rsplitn(3)
    // returns at most 3 slices with the rightmost 2 separator positions
    // driving the split; any additional underscores stay in the leading
    // slice (the name segment... wait, no — the leading slice of a rsplitn
    // iterator is the LAST returned item. So the iteration order gives
    // arch first, version second, name third).
    let mut parts_iter = stem.rsplitn(3, '_');
    let arch = parts_iter.next()?;
    let version = parts_iter.next()?;
    let name = parts_iter.next()?;
    if name.is_empty() || version.is_empty() || arch.is_empty() {
        return None;
    }
    Some((name.to_string(), version.to_string(), arch.to_string()))
}
```

### 2.4 Semantic decision matrix

| Input filename                                                  | Pre-m185 result                                        | Post-m185 result                                    | Change? |
|------------------------------------------------------------------|--------------------------------------------------------|-----------------------------------------------------|---------|
| `test-pkg_1.0-r0_all.ipk`                                        | `Some(("test-pkg", "1.0-r0", "all"))`                  | `Some(("test-pkg", "1.0-r0", "all"))`               | NONE (canonical 2-underscore) |
| `packagegroup-core-boot_1.0-r0_all.ipk`                          | `Some(("packagegroup-core-boot", "1.0-r0", "all"))`    | Same                                                | NONE (canonical, `-` in name) |
| `test-pkg_1.0+git0+abc_def-r0_all.ipk`                           | `None` (parts.len() == 4)                              | `Some(("test-pkg", "1.0+git0+abc_def-r0", "all"))` ✱ | ✱ CHANGED (multi-underscore version) |
| `kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk` | `None` (parts.len() == 4) | `Some(("kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard", "6.6.127+git0+45f69741c7_70af2998be-r0", "qemux86_64"))` ✱ | ✱ CHANGED (real Yocto shape) |
| `no-underscores.ipk`                                             | `None` (parts.len() == 1)                              | `None` (rsplitn produces 1 slice)                   | NONE (malformed) |
| `single_underscore.ipk`                                          | `None` (parts.len() == 2)                              | `None` (rsplitn produces 2 slices → name missing)   | NONE (malformed) |
| `test-pkg_1.0-r0_all` (no `.ipk` suffix)                         | `None` (strip_suffix fails)                            | `None` (strip_suffix fails)                         | NONE (extension missing) |
| `_1.0-r0_all.ipk` (empty name)                                   | `None` (parts[0].is_empty())                           | `None` (name.is_empty() guard fires)                | NONE (fail-safe) |
| `test-pkg__all.ipk` (empty version)                              | `None` (parts[1].is_empty())                           | `None` (version.is_empty() guard fires)             | NONE (fail-safe) |
| `test-pkg_1.0-r0_.ipk` (empty arch)                              | `None` (parts[2].is_empty())                           | `None` (arch.is_empty() guard fires)                | NONE (fail-safe) |

✱ = new m185 behavior. Two rows CHANGED (both multi-underscore versions). All other rows preserve pre-m185 behavior byte-identically.

## 3. US2 — Opkg License Wiring

### 3.1 Function under change

**File**: `mikebom-cli/src/scan_fs/package_db/opkg.rs`
**Function**: `fn build_entry` at line 203
**Site of change**: line 289 (`licenses: Vec::new(),` → `licenses,`)

### 3.2 Post-m185 pipeline

Inside `build_entry`, BEFORE the `Some(PackageDbEntry { ... })` construction, add:

```rust
// Milestone 185 US2 (#539) — extract License from stanza + normalize.
// Pipeline mirrors rpm_file.rs's 3-pass with an added 4th-pass wholesale-
// wrap fallback (per m185 Q1 clarification / FR-014).
use mikebom_common::types::license::SpdxExpression;
let licenses: Vec<SpdxExpression> = stanza
    .license()
    .filter(|l| !l.trim().is_empty())  // whitespace-only treated as absent
    .and_then(|raw| {
        // Pass 1: normalize BitBake `&`/`|` → SPDX `AND`/`OR`.
        let normalized = super::rpm_file::normalize_bitbake_license_operators(raw);
        // Pass 2: strict SPDX try_canonical.
        if let Ok(e) = SpdxExpression::try_canonical(&normalized) {
            return Some(e);
        }
        // Pass 3: preserve_known_operands_with_license_ref (rpm's #481
        // per-operand LicenseRef wrap) + re-canonicalize.
        if let Some(wrapped) = super::rpm_file::preserve_known_operands_with_license_ref(&normalized) {
            if let Ok(e) = SpdxExpression::try_canonical(&wrapped) {
                return Some(e);
            }
        }
        // Pass 4 (m185 US2 wholesale-wrap, opkg-only) — wrap the WHOLE
        // original string as a single LicenseRef-<sanitized> operand.
        // Preserves the raw data for downstream license auditors. Emits
        // a tracing::warn! so operators can see which stanzas hit this
        // fallback.
        let sanitized = super::rpm_file::sanitize_to_license_ref_idstring(raw)?;
        let wrapped = format!("LicenseRef-{sanitized}");
        tracing::warn!(
            source_path = %source_path,
            package = %name,
            raw_license = %raw,
            wrapped = %wrapped,
            "opkg License string failed strict + per-operand SPDX parse; \
             wholesale-wrapped as LicenseRef per m185 FR-014"
        );
        SpdxExpression::try_canonical(&wrapped).ok()
    })
    .into_iter()
    .collect();
```

Then replace line 289's `licenses: Vec::new(),` with `licenses,` (consuming the variable).

### 3.3 Pipeline decision matrix

| `stanza.license()` return value                | Pass 1 (op normalize)              | Pass 2 (try_canonical)                    | Pass 3 (per-operand wrap)                | Pass 4 (wholesale-wrap)                | Emitted licenses                     |
|-------------------------------------------------|------------------------------------|-------------------------------------------|------------------------------------------|----------------------------------------|-------------------------------------|
| `None` (field absent)                          | (skipped)                          | (skipped)                                 | (skipped)                                | (skipped)                              | `Vec::new()` (unchanged)             |
| `Some("")`                                     | (whitespace filter drops)          | (skipped)                                 | (skipped)                                | (skipped)                              | `Vec::new()` (whitespace-only)       |
| `Some("   ")`                                  | (whitespace filter drops)          | (skipped)                                 | (skipped)                                | (skipped)                              | `Vec::new()` (whitespace-only)       |
| `Some("GPL-2.0-only")`                         | `"GPL-2.0-only"` (unchanged)       | `Ok(SpdxExpression)` — DONE                | (skipped)                                | (skipped)                              | `vec![SpdxExpression("GPL-2.0-only")]` |
| `Some("GPL-2.0-only AND MIT")`                 | (unchanged — no `&`/`|`)           | `Ok(SpdxExpression)` — DONE                | (skipped)                                | (skipped)                              | `vec![SpdxExpression(...)]`          |
| `Some("GPLv2 & bzip2-1.0.4")`                  | `"GPLv2 AND bzip2-1.0.4"`          | `Err` (GPLv2 not canonical, bzip2 unknown) | `"GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` → `Ok` | (skipped) | `vec![SpdxExpression(...)]`          |
| `Some("Apache-2.0")`                           | (unchanged)                        | `Ok(SpdxExpression)` — DONE                | (skipped)                                | (skipped)                              | Runtime — DONE at pass 2             |
| `Some("!!! broken &&& syntax !!!")`             | (nothing to normalize)             | `Err` (invalid syntax)                    | `None` (unable to tokenize into valid SPDX shape) | `LicenseRef-------broken----syntax------` → `Ok` | `vec![SpdxExpression("LicenseRef-...")]` ✱ |
| `Some("!")` (single non-idstring char)         | (unchanged)                        | `Err`                                     | `None`                                   | `sanitize_to_license_ref_idstring` returns `None` (no idstring-safe chars) | `Vec::new()` (fail-safe, matches FR-007) |

✱ = m185 wholesale-wrap fallback fires (only when passes 1–3 all fail AND sanitization produces a non-empty idstring).

### 3.4 rpm-side non-modification invariant

**File**: `mikebom-cli/src/scan_fs/package_db/rpm_file.rs`
**Changes**: 3 function visibility bumps (`fn` → `pub(crate) fn`):
- Line 615: `normalize_bitbake_license_operators`
- Line 832: `preserve_known_operands_with_license_ref`
- Line 770: `sanitize_to_license_ref_idstring`

**Behavior change**: NONE. The rpm reader's call site (`rpm_file.rs:469-488`) does NOT invoke the 4th-pass wholesale-wrap. rpm keeps its 3-pass pipeline unchanged. Rpm goldens MUST stay byte-identical per SC-005.

## 4. Test Contract

**Unit tests** (colocated with the code they cover):

**ipk_file.rs::tests** (new):
- `parse_ipk_filename_canonical_2underscore_still_parses` (regression pin — canonical case)
- `parse_ipk_filename_multi_underscore_version_now_parses` (US1 acceptance 1 — the fix)
- `parse_ipk_filename_yocto_kernel_module_shape` (US1 acceptance 4 — real Yocto shape)
- `parse_ipk_filename_no_ipk_suffix_still_none` (regression pin — extension missing)
- `parse_ipk_filename_no_underscores_still_none` (regression pin — malformed)
- `parse_ipk_filename_empty_field_still_none` (regression pin — empty-guard)

**opkg.rs::tests** (new):
- `build_entry_extracts_canonical_spdx_license` (US2 acceptance 1)
- `build_entry_bitbake_operator_normalizes_and_wraps_unknown_operand` (US2 acceptance 2)
- `build_entry_absent_license_stays_empty` (US2 acceptance 3 — regression pin)
- `build_entry_whitespace_only_license_treated_as_absent` (edge case)
- `build_entry_unparseable_license_wholesale_wraps` (m185 FR-014 — US2 4th-pass fallback)
- `build_entry_unsanitizable_license_falls_through_to_empty` (m185 FR-014 defensive edge — unsanitizable input still emits `licenses: Vec::new()`)

**Integration tests** (via existing regression fixtures):
- Existing ipk regression tests (m169 US1–US6 in `ipk_file.rs::tests`) MUST continue to pass byte-identically. m185's `rsplitn(3, '_')` produces the same output as pre-m185 `split('_')` for canonical 2-underscore filenames.
- Existing opkg regression tests (m107 in `opkg.rs::tests`) MUST continue to pass, allowing for additive changes on any test whose stanza carries a License field (m185 now populates `licenses` for those).
- Existing rpm regression tests (m152 + m165 + m168 audit-harness family) MUST continue to pass byte-identically — rpm behavior unchanged per Decision 2's FR-011 invariant.

**Golden regen expected shape**:
- Non-Yocto goldens: ZERO drift (SC-005/SC-006).
- rpm goldens: ZERO drift (FR-011 rpm-side invariant).
- opkg / ipk goldens (if any exist and exercise the m185 signals): additive changes only — new `licenses[]` entries where fixture stanzas carry License fields; corrected `purl` values where fixture filenames use multi-underscore versions.

## 5. Backward Compatibility

- **Canonical 2-underscore ipk filenames**: byte-identical output (SC-004 gate, US1 acceptance 2).
- **Opkg stanzas WITHOUT License field**: byte-identical output (`licenses: []` preserved per FR-007).
- **Opkg stanzas with License containing canonical SPDX expressions**: NEW output (pre-m185 was `licenses: []`, post-m185 emits `licenses: [SpdxExpression(...)]`). This is the m185 US2 fix, additive-only change.
- **rpm goldens across all formats**: byte-identical (FR-011 rpm-side non-modification invariant).
