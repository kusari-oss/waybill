# Data Model: milestone 169 — ipk archive-file reader + opkg installed-DB hardening

**Date**: 2026-07-06
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase 1 data model. m169 introduces one new module + edits one existing module + one walker allowlist entry + one dispatcher wire-up. No new external wire types.

## E1 — `ipk_file.rs` NEW module

**Location**: `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`

### Entry point

```rust
/// Walk `<rootfs>` for `.ipk` files; emit one `PackageDbEntry` per
/// well-formed OR filename-parseable archive. Per FR-006/FR-007, every
/// skipped file fires a WARN tracing line.
///
/// The `IpkReaderConfig` threads env-var-driven behavior (matches m069
/// `RpmReaderConfig` precedent) without polluting the shared read_all
/// signature.
pub fn read(
    rootfs: &Path,
    distro_version: Option<&str>,
    config: &IpkReaderConfig,
) -> Vec<PackageDbEntry>;

/// Populate the binary walker's claim set with paths declared in each
/// `.ipk`'s `data.tar.gz` payload — mirrors `dpkg::collect_claimed_paths`
/// + `opkg::collect_claimed_paths`.
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut HashSet<PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut HashSet<(u64, u64)>,
);
```

### `IpkReaderConfig`

```rust
pub struct IpkReaderConfig {
    /// Cap on uncompressed `control.tar.gz` size — anything above emits
    /// filename-only + `mikebom:archive-size-skipped` annotation per
    /// FR-012. Default 16 MB (matches m069 RPM cap).
    pub max_control_size: u64,
}

impl Default for IpkReaderConfig {
    fn default() -> Self {
        Self { max_control_size: 16 * 1024 * 1024 }
    }
}
```

### Internal helpers (private)

```rust
/// Parse the ar-archive envelope. Returns entry offsets + sizes so the
/// caller can extract control.tar.gz or data.tar.gz by slice.
///
/// Ar format (research §R2): 8-byte magic `!<arch>\n` + repeating
/// 60-byte per-entry headers. Hand-rolled per Constitution I.
fn parse_ar_envelope(bytes: &[u8]) -> Result<Vec<ArEntry>, IpkParseError>;

struct ArEntry { name: String, offset: usize, size: usize }

/// Extract `control.tar.gz` from an ar-envelope's raw bytes, gunzip +
/// untar, and return the control file's contents as a UTF-8 string.
/// Uses existing `flate2` + `tar` from the workspace.
fn extract_control_file(ar_bytes: &[u8], entry: &ArEntry) -> Result<String, IpkParseError>;

/// Extract the file-list from `data.tar.gz` for `collect_claimed_paths`.
fn extract_data_file_list(ar_bytes: &[u8], entry: &ArEntry) -> Result<Vec<String>, IpkParseError>;

/// Filename fallback per US2 / FR-006 — parses `<name>_<version>_<arch>.ipk`.
/// Returns None for filenames not matching the canonical form.
fn parse_ipk_filename(filename: &str) -> Option<(String, String, String)>;

enum IpkParseError {
    ArMalformed(String),
    ControlMissing,
    ControlUnreadable(std::io::Error),
    ControlOversize { actual: u64, cap: u64 },
    DataMissing,
    FilenameNonConforming,
}
```

### Emission per `.ipk`

For each `.ipk` file the walker hands to `ipk_file::read`:

1. Read file bytes; parse ar envelope via `parse_ar_envelope`.
2. On success: extract control.tar.gz → parse via `parse_stanzas` (shared from `control_file.rs`) → derive fields per E4 below.
3. On failure: try filename fallback via `parse_ipk_filename`.
4. Emit a `PackageDbEntry` populated per E4 (below).
5. On both failures: log WARN naming file + parse-failure class; do NOT emit.

## E2 — `opkg.rs` EDITED (m107 hardening deltas)

**Location**: `mikebom-cli/src/scan_fs/package_db/opkg.rs`

### Delta 1 — FR-014 fallback to `info/*.control`

**Current** (`opkg.rs:79-80`):
```rust
if !status_path.is_file() {
    return (Vec::new(), ctx);
}
```

**Post-169**:
```rust
if !status_path.is_file() {
    // FR-014: /var/lib/opkg/status absent → fall back to
    // enumerating /var/lib/opkg/info/*.control per-package files.
    let info_dir = rootfs.join(OPKG_INFO_DIR);
    if info_dir.is_dir() {
        tracing::info!(
            info_dir = %info_dir.display(),
            "opkg installed-DB: status file absent; falling back to info/*.control per FR-014"
        );
        let out = parse_info_dir_fallback(&info_dir, &ctx);
        return (out, ctx);
    }
    return (Vec::new(), ctx);
}
```

Plus new private helper `parse_info_dir_fallback` that enumerates `info/*.control`, calls `parse_stanzas` on each, and emits `PackageDbEntry` per stanza.

### Delta 2 — FR-015 evidence-kind

**Current** (`opkg.rs:203`):
```rust
evidence_kind: None,
```

**Post-169**:
```rust
evidence_kind: Some("opkg-status-db".to_string()),
```

Applied to BOTH the `parse` function's PackageDbEntry emission AND the new `parse_info_dir_fallback` helper's emissions.

### Delta 3 — FR-005 alternative-list handling (shared with ipk_file.rs)

The `Depends:` field parsing in `opkg::parse` currently splits on `,` only. Post-169 it must ALSO handle `|` per Q2. **Delta**: apply the alternative-list treatment (first-wins + `mikebom:dep-alternative-alternates` annotation) inside the `Depends:` parsing path.

This treatment SHOULD be shared: `ipk_file.rs` needs the same alternative-list handling. Extract as a private helper `parse_depends_field_with_alternatives(raw: &str) -> DepsWithAlternatives` in a new module `depends_alternatives.rs` under `package_db/` (or add to `control_file.rs`). Both `opkg.rs` and `ipk_file.rs` consume it.

## E3 — Dispatcher wire-up (`package_db/mod.rs`)

**Location**: `mikebom-cli/src/scan_fs/package_db/mod.rs`

### Delta 1 — Register module

Add near existing module declarations:

```rust
pub mod ipk_file;
```

### Delta 2 — Wire into `read_all`

Add after existing `rpm_file::read(...)` extension (per Explore agent finding at `mod.rs:1505`):

```rust
// Milestone 169 (US1): .ipk archive-file reader. Sits at the same
// tier as rpm_file (both are artifact-file readers, distinct from
// installed-DB readers).
let ipk_config = ipk_file::IpkReaderConfig::default();
out.extend(ipk_file::read(rootfs, distro_version.as_deref(), &ipk_config));
```

Also register the `collect_claimed_paths` call so binary-walker skip-set includes ipk-declared files:

```rust
ipk_file::collect_claimed_paths(rootfs, &mut claimed_paths, #[cfg(unix)] &mut claimed_inodes);
```

### Delta 3 — FR-016 dedup precedence (installed-DB wins over archive-file)

After the `out.extend(...)` calls that add BOTH archive-file components AND installed-DB components, insert a dedup pass keyed by `(purl.as_str())` that removes archive-file entries whose PURL already appears from an installed-DB entry. Preserve the installed-DB entry unchanged.

Location: after all package_db reader calls, before returning `out`. Implementation: split into two `Vec<PackageDbEntry>` — one from installed-DB readers (dpkg, apk, rpm, alpm, opkg) + one from archive-file readers (rpm_file, ipk_file) — then merge with installed-DB taking precedence on PURL collision.

## E4 — `PackageDbEntry` population fields

For both `ipk_file::read` (archive-file source) + `opkg::read` (installed-DB source), the `PackageDbEntry` per-emission population:

| Field | ipk_file source | opkg installed-DB source |
|---|---|---|
| `name` | control file `Package:` field, or filename first segment on fallback | control file `Package:` field |
| `version` | control file `Version:` field, or filename second segment on fallback | control file `Version:` field |
| `arch` | control file `Architecture:` field, or filename third segment on fallback | control file `Architecture:` field |
| `source_path` | absolute path to the `.ipk` file | absolute path to `/var/lib/opkg/status` (or the `info/*.control` file when FR-014 fallback) |
| `depends` | parsed `Depends:` field with Q2 alternative-list treatment (first wins) | same |
| `purl` | `pkg:opkg/<name>@<version>?arch=<arch>` via `Purl::new` | same |
| `licenses` | parsed `License:` field routed through `SpdxExpression::try_canonical` + m152 LicenseRef escape hatch | same |
| `evidence_kind` | `Some("ipk-file".to_string())` per FR-009 | `Some("opkg-status-db".to_string())` per FR-015 |
| `sbom_tier` | `Some("analyzed".to_string())` (archive-file = analyzed tier per m106 convention) | `Some("deployed".to_string())` (installed-DB = deployed tier) |
| `extra_annotations` | `mikebom:dep-alternative-alternates` when `Depends:` alternatives detected; `mikebom:archive-size-skipped` when over the cap | `mikebom:dep-alternative-alternates` when applicable; `mikebom:opkg-status-fallback` when FR-014 fallback fired |
| `maintainer` | control file `Maintainer:` field | control file `Maintainer:` field |

## E5 — Wire annotations (parity-catalog values)

**Existing C50 `mikebom:evidence-kind` row** (parity catalog) — m169 adds two new legal values:

- `"ipk-file"` — new (m169 FR-009) — parity with `rpm-file` (m004)
- `"opkg-status-db"` — new (m169 FR-015) — parity with `rpmdb-sqlite` (m004)

**NEW annotation** `mikebom:dep-alternative-alternates` (Q2 clarification):

- Wire shape: JSON array of package names (e.g., `["libmbedtls-12", "libssl3"]`)
- Emitted on the SOURCE component whose `Depends:` field contained the alternative-list
- Consumed by consumers wanting fallback visibility without BFS reachability inflation
- Extends the existing parity-catalog C48 `mikebom:resolver-step` value list (or a new row TBD — plan for TBD parity-catalog decision in the tasks phase)

**Existing annotations reused**:
- `mikebom:archive-size-skipped` (FR-012) — reused from m069 rpm cap parity
- `mikebom:opkg-status-fallback` (FR-014 new — small addition) — indicates the reader took the info/*.control fallback path

## E6 — File-tier walker allowlist edit

**Location**: `mikebom-cli/src/scan_fs/file_tier/content_shape.rs`

Delta: add `.ipk` to the recognized-artifact-suffix allowlist (implementation detail — likely a `const` array or match arm; the exact edit lives in the tasks phase). FR-001 requires this single change.

## Wire types

None new. All emitted output uses existing SBOM wire formats:

- CDX 1.6: components carry `pkg:opkg/*` PURL + `properties[]` with `mikebom:evidence-kind` + optional `mikebom:dep-alternative-alternates` + optional `mikebom:archive-size-skipped`. `dependsOn` edges follow first-alternative-only semantic per Q2.
- SPDX 2.3: `packages[]` with `externalRefs[type:purl]` + `annotations[]` for the mikebom-annotation envelope.
- SPDX 3.0.1: `Package` elements with `software_packageUrl` + `Annotation` elements.

## Relationships

```text
walk_file_tier (m133 walker)
    ↓ recognizes `.ipk` (FR-001, E6 delta)
    ↓
ipk_file::read (E1) [NEW]
    ↓ parses ar-envelope (hand-rolled, R2)
    ↓ extracts control.tar.gz → parse_stanzas (m107 shared, R3)
    ↓ falls back to filename on parse failure (US2)
    ↓ emits Vec<PackageDbEntry> with evidence_kind = "ipk-file"

package_db/mod.rs::read_all (E3 delta)
    ↓ extends out: Vec<PackageDbEntry> with ipk_file emissions
    ↓ FR-016 dedup: installed-DB wins over archive-file on PURL collision

opkg::read (E2 delta)  [m107 EXISTING + m169 hardening]
    ↓ /var/lib/opkg/status parse (m107) or info/*.control fallback (FR-014, new)
    ↓ evidence_kind = "opkg-status-db" (FR-015, new)
    ↓ Depends alternative-list = first-wins + mikebom:dep-alternative-alternates (Q2, new)
    ↓ emits Vec<PackageDbEntry>

collect_claimed_paths (m104 binary-walker skip-set)
    ↓ receives skip-set from ipk_file (FR-011 new) + opkg (m107 existing)
    ↓ binary walker skips files claimed by either source
```

## State transitions

None. m169 is a pure read-path addition — no evolving state across scan lifetimes.

## Data volume assumptions

- Per Yocto scarthgap `core-image-minimal` build: 4587 `.ipk` files → ~4580 `PackageDbEntry` emissions (small tolerance for malformed).
- Per OpenWrt runtime rootfs: 36 installed packages (per SC-005b anchor) → 36 `PackageDbEntry` emissions from opkg installed-DB path.
- Memory: PackageDbEntry ~500 bytes each; 4587 entries = ~2.3 MB peak. Well within `--offline` mikebom's typical memory footprint.
- Wall-clock: ar-envelope parse is O(1) per file (fixed-offset header reads); control.tar.gz decompression dominates. 4587 files × ~100 KB avg = ~450 MB total gzip work. Modern hardware processes this in <10s. SC-011 attests to actual wall-clock in the PR body.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| Every `.ipk` file the walker sees produces a component OR a WARN (never silent skip) | FR-006/FR-007 in ipk_file::read; verified by SC-009 tests |
| Emitted PURLs are `pkg:opkg/<name>@<version>?arch=<arch>` per purl-spec | FR-004 via `Purl::new` construction; verified by SC-002 |
| License field routes through `SpdxExpression::try_canonical` + m152 escape hatch | FR-008 in ipk_file::read + opkg::parse |
| Alternative-list `Depends:` emits first-only + annotation | Q2 clarification via shared `parse_depends_field_with_alternatives`; verified by SC-009(m) test |
| Installed-DB wins over archive-file on PURL collision | FR-016 in read_all dedup pass; verified by SC-005b + integration test |
| `.ipk` files NOT in the walker allowlist before m169 → after m169: 0 silent shape-skips | FR-001 (walker allowlist edit); verified by SC-001 empirical anchor |
