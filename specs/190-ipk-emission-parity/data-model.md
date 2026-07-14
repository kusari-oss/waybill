# Data Model: m190 ipk Emission Parity

**Date**: 2026-07-13
**Scope**: In-process types touched by the three fixes. All types live in `mikebom-cli` (no `mikebom-common` additions per Constitution Principle VI).

## Entity: ParsedControlRecord (existing, EXTENDED)

Owns the parsed `.ipk` control-file metadata. Located in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`.

### New field

```rust
/// Milestone 190 (issue #552): epoch parsed from Debian-style
/// `<digits>:<version>` prefix in the control-file `Version:` field.
/// `None` when no prefix present; `Some(0)` is retained through the
/// parser but the PURL emitter treats it identically to `None` per
/// purl-spec convention (see `build_opkg_purl`).
pub epoch: Option<u32>,
```

**Existing fields** (unchanged): `name: String`, `version: String` (post-fix: naked upstream version with `<digits>:` prefix stripped), `arch: String`, `license: Option<String>`, `depends_field: String`, `data_file_list: Vec<String>`.

### Validation rules

- `epoch: Option<u32>`: parsed only from `^(\d+):(.*)$` regex match on the raw `Version:` field. Non-digit prefixes are NOT treated as epochs; they remain in the `version` string as-is.
- `version: String`: post-fix, contains only the naked upstream form (no `<digits>:` prefix). If the raw `Version:` had no epoch prefix, `version` is byte-identical to pre-fix behavior.

### State transitions

Not stateful — the record is constructed once per ipk during parse and passed through to `assemble_entry`.

## Function: normalize_bitbake_license_operators (NEW)

**Signature**:

```rust
/// Milestone 190 (issue #550): normalize BitBake license operators
/// (`&`, `&&`, `|`, `||`) to their SPDX equivalents (`AND`, `OR`)
/// so the raw ipk `License:` field can be passed to
/// `SpdxExpression::try_canonical` without losing to the lenient
/// `SpdxExpression::new` fallback.
///
/// Ordering invariant: MUST substitute long-form (`&&`, `||`) before
/// single-form (`&`, `|`) to avoid partial-token consumption. The four
/// `str::replace` calls below encode this order; do NOT reorder without
/// re-reading spec §Q1 + research §R1.
///
/// Whitespace: this function adds a single space on each side of the
/// SPDX operator; `SpdxExpression::try_canonical` collapses runs of
/// whitespace during parse, so `MIT&&Apache-2.0` (no whitespace)
/// becomes `MIT  AND  Apache-2.0` here and canonicalizes fine.
fn normalize_bitbake_license_operators(raw: &str) -> String
```

**Location**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs` (co-located with the ipk reader; not exported).

**Behavior**:
- Input: any string; typically the raw content of the `License:` control field.
- Output: the input with `&&` → ` AND `, `||` → ` OR `, `&` → ` AND `, `|` → ` OR ` applied in that order.
- Pure function; no allocation beyond the returned `String`.
- Preserves parenthesization + `WITH`-clauses verbatim (only operator tokens are substituted).

**Invariants**:
- Idempotent: applying twice equals applying once (since `AND`/`OR` contain no `&`/`|` characters).
- Whitespace-insensitive on the input side: any spacing around the BitBake operator is tolerated.

## Function: parse_opkg_version_with_epoch (NEW)

**Signature**:

```rust
/// Milestone 190 (issue #552): parse the raw `Version:` field into
/// (optional epoch, naked-version) per the Debian/opkg convention
/// where `<digits>:<version>-<release>` embeds the epoch inline.
///
/// Returns `(None, raw.to_string())` when no `<digits>:` prefix is
/// present, preserving byte-identity for non-epoch inputs (SC-006).
///
/// # Examples
/// ```
/// assert_eq!(parse_opkg_version_with_epoch("1:2.0-r0"), (Some(1), "2.0-r0".into()));
/// assert_eq!(parse_opkg_version_with_epoch("2.0-r0"),   (None,    "2.0-r0".into()));
/// assert_eq!(parse_opkg_version_with_epoch("0:1.0-r0"), (Some(0), "1.0-r0".into()));
/// ```
fn parse_opkg_version_with_epoch(raw: &str) -> (Option<u32>, String)
```

**Location**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs` (co-located with the ipk reader; not exported).

**Behavior**:
- Regex `^(\d+):(.*)$` applied via `regex::Regex` compiled via `std::sync::OnceLock`.
- On match: parse capture-group-1 as `u32`, return `(Some(u32), capture_group_2.to_string())`.
- On non-match OR parse failure (overflow, etc.): return `(None, raw.to_string())`.
- Multi-colon input (`1:2.0-r0:beta`): first `<digits>:` only is treated as epoch; rest of string preserved verbatim in the returned naked-version.

**Invariants**:
- Idempotent-modulo-tuple: applying to the naked-version output yields `(None, same_string)`.
- Returns the exact input string when no epoch prefix present (SC-006 byte-identity gate).

## Function: build_opkg_purl (existing, EXTENDED)

**Location**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:1087+`.

**Existing signature** (approximate, unchanged): `fn build_opkg_purl(name: &str, version: &str, arch: &str, distro_tag: Option<&str>) -> Result<Purl, IpkParseError>`

**Extended signature**:

```rust
fn build_opkg_purl(
    name: &str,
    version: &str,       // POST-FIX: naked version, no <digits>: prefix
    arch: &str,
    distro_tag: Option<&str>,
    epoch: Option<u32>,  // NEW: from parse_opkg_version_with_epoch
) -> Result<Purl, IpkParseError>
```

**Behavior extension**:
- If `epoch == Some(v) && v != 0`: append `&epoch=<v>` qualifier to the PURL, positioned alphabetically per purl-spec §5.6 (after `arch=` and `distro=` if present — `arch < distro < epoch` alphabetically).
- If `epoch == None` OR `epoch == Some(0)`: NO qualifier change; output byte-identical to pre-fix.

**Invariant preservation**:
- FR-011 byte-identity: for any `(name, version, arch, distro_tag, None)` OR `(name, version, arch, distro_tag, Some(0))` input, the produced PURL string is byte-identical to the pre-fix `build_opkg_purl(name, version, arch, distro_tag)` output.

## Function: ipk `parse_control_stanza` (existing, EXTENDED emission-side)

**Location**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:800+` (function name may vary; the code block around line 820 in the current tree).

**Extended behavior**: The current logic at line 824-840 becomes:

```rust
let licenses = match stanza.get("license") {
    Some(raw) if !raw.trim().is_empty() => {
        // NEW (m190 US1 / #550): normalize BitBake operators BEFORE canonicalization.
        let normalized = normalize_bitbake_license_operators(raw);
        match mikebom_common::types::license::SpdxExpression::try_canonical(&normalized) {
            Ok(e) => vec![e],
            Err(_) => {
                match mikebom_common::types::license::SpdxExpression::new(&normalized) {
                    Ok(e) => vec![e],
                    Err(_) => Vec::new(),
                }
            }
        }
    }
    _ => Vec::new(),
};

// NEW (m190 US3 / #552): extract epoch from version field.
let (epoch, naked_version) = parse_opkg_version_with_epoch(&version);
// Continue with `naked_version` and `epoch` in place of `version`.
```

**Rationale**: Zero-touch to unrelated fields; changes are surgically confined to the license and version parsing steps.

## Emission-shape contracts (see `contracts/`)

The changes above alter the wire-shape of emitted CDX, SPDX 2.3, and SPDX 3 JSON in specific ways. See `contracts/emission-shape.md` for the byte-level shape contracts each format must satisfy post-milestone.
